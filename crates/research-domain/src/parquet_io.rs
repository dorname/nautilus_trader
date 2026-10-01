//! 行情 Parquet 分区读写（架构「数据模型」节）。
//!
//! 分区 schema：instrument_id、trade_date、open/high/low/close（十进制定点）、
//! volume_shares、amount_cny、price_basis、source、available_at。
//! ingested_at 属于执行元数据（写入 SQLite snapshot.created_at），不进入
//! 被哈希的分区内容，保证相同源数据重复导入产生相同分区字节与哈希。

use std::{fs::File, path::Path, sync::Arc};

use arrow::{
    array::{
        Array, ArrayRef, Date32Array, Decimal128Array, Int64Array, RecordBatch, StringArray,
        TimestampNanosecondArray,
    },
    datatypes::{DataType, Field, Schema, TimeUnit},
};
use parquet::arrow::{arrow_reader::ParquetRecordBatchReaderBuilder, ArrowWriter};
use rust_decimal::Decimal;

use crate::{
    error::{ErrorCode, ResearchError, Result},
    protocol::{ImportSource, PriceBasis},
    quotes::QuoteRow,
    time::{date32_to_trade_date, trade_date_to_date32},
};

const PRICE_SCALE: u32 = 4;

fn quotes_schema() -> Arc<Schema> {
    Arc::new(Schema::new(vec![
        Field::new("instrument_id", DataType::Utf8, false),
        Field::new("trade_date", DataType::Date32, false),
        Field::new("open", DataType::Decimal128(18, PRICE_SCALE as i8), false),
        Field::new("high", DataType::Decimal128(18, PRICE_SCALE as i8), false),
        Field::new("low", DataType::Decimal128(18, PRICE_SCALE as i8), false),
        Field::new("close", DataType::Decimal128(18, PRICE_SCALE as i8), false),
        Field::new("volume_shares", DataType::Int64, false),
        // 成交额可空：缺失是真实数据形态（UT-S12-05）
        Field::new("amount_cny", DataType::Decimal128(18, PRICE_SCALE as i8), true),
        Field::new("price_basis", DataType::Utf8, false),
        Field::new("source", DataType::Utf8, false),
        Field::new(
            "available_at",
            DataType::Timestamp(TimeUnit::Nanosecond, Some("UTC".into())),
            false,
        ),
    ]))
}

fn to_scaled(dec: &Decimal) -> Result<i128> {
    let rounded = dec.round_dp(PRICE_SCALE);
    if rounded != *dec {
        return Err(ResearchError::invalid(format!(
            "价格超出小数精度（{PRICE_SCALE} 位）：{dec}"
        )));
    }
    // round_dp 不补零：mantissa 需按实际 scale 对齐到 PRICE_SCALE
    let scale = rounded.scale();
    Ok(rounded.mantissa() * 10i128.pow(PRICE_SCALE - scale))
}

/// 收盘后可见时点：交易日 15:00（Asia/Shanghai）= 07:00 UTC，确定性推导。
fn available_at_ns(trade_date: &str) -> Result<i64> {
    let days = i64::from(trade_date_to_date32(trade_date)?);
    Ok((days * 86_400 + 7 * 3600) * 1_000_000_000)
}

/// 将同一标的的行情行写入一个 Parquet 分区文件（先排序，写完 fsync）。
pub fn write_quotes_partition(
    path: &Path,
    rows: &[QuoteRow],
    basis: PriceBasis,
    source: ImportSource,
) -> Result<()> {
    let mut rows = rows.to_vec();
    rows.sort_by(|a, b| a.trade_date.cmp(&b.trade_date));

    let schema = quotes_schema();
    let n = rows.len();
    let mut instruments = Vec::with_capacity(n);
    let mut dates = Vec::with_capacity(n);
    let (mut opens, mut highs, mut lows, mut closes, mut amounts) =
        (Vec::with_capacity(n), Vec::with_capacity(n), Vec::with_capacity(n), Vec::with_capacity(n), Vec::with_capacity(n));
    let mut volumes = Vec::with_capacity(n);
    let mut available = Vec::with_capacity(n);
    for r in &rows {
        instruments.push(r.instrument_id.clone());
        dates.push(trade_date_to_date32(&r.trade_date)?);
        opens.push(to_scaled(&r.open)?);
        highs.push(to_scaled(&r.high)?);
        lows.push(to_scaled(&r.low)?);
        closes.push(to_scaled(&r.close)?);
        amounts.push(match &r.amount_cny {
            Some(a) => Some(to_scaled(a)?),
            None => None,
        });
        volumes.push(r.volume_shares as i64);
        available.push(available_at_ns(&r.trade_date)?);
    }

    let decimal = |vals: Vec<i128>| -> Result<ArrayRef> {
        Ok(Arc::new(
            Decimal128Array::from_iter_values(vals)
                .with_precision_and_scale(18, PRICE_SCALE as i8)
                .map_err(|e| ResearchError::invalid(format!("Decimal128 精度错误：{e}")))?,
        ))
    };
    // 可空十进制列（成交额缺失）
    let decimal_opt = |vals: Vec<Option<i128>>| -> Result<ArrayRef> {
        Ok(Arc::new(
            Decimal128Array::from(vals)
                .with_precision_and_scale(18, PRICE_SCALE as i8)
                .map_err(|e| ResearchError::invalid(format!("Decimal128 精度错误：{e}")))?,
        ))
    };

    let batch = RecordBatch::try_new(
        schema.clone(),
        vec![
            Arc::new(StringArray::from(instruments)),
            Arc::new(Date32Array::from(dates)),
            decimal(opens)?,
            decimal(highs)?,
            decimal(lows)?,
            decimal(closes)?,
            Arc::new(Int64Array::from(volumes)),
            decimal_opt(amounts)?,
            Arc::new(StringArray::from(vec![basis.as_str().to_string(); n])),
            Arc::new(StringArray::from(vec![source.as_str().to_string(); n])),
            Arc::new(TimestampNanosecondArray::from_iter_values(available).with_timezone("UTC")),
        ],
    )
    .map_err(|e| ResearchError::invalid(format!("构建行情批次失败：{e}")))?;

    let file = File::create(path).map_err(|e| map_io("创建分区文件", e))?;
    let sync_handle = file.try_clone().map_err(|e| map_io("复制文件句柄", e))?;
    let mut writer = ArrowWriter::try_new(file, schema, None)
        .map_err(|e| ResearchError::invalid(format!("创建 Parquet 写入器失败：{e}")))?;
    writer
        .write(&batch)
        .map_err(|e| ResearchError::invalid(format!("写入 Parquet 失败：{e}")))?;
    writer
        .close()
        .map_err(|e| ResearchError::invalid(format!("关闭 Parquet 文件失败：{e}")))?;
    sync_handle.sync_all().map_err(|e| map_io("同步分区文件", e))?;
    Ok(())
}

/// 读取一个分区文件的全部行情行（按 trade_date 升序返回）。
pub fn read_quotes_partition(path: &Path) -> Result<Vec<QuoteRow>> {
    let file = File::open(path).map_err(|e| map_io("打开分区文件", e))?;
    let reader = ParquetRecordBatchReaderBuilder::try_new(file)
        .map_err(|e| ResearchError::new(ErrorCode::CorruptArtifact, format!("Parquet 头读取失败：{e}")))?
        .build()
        .map_err(|e| ResearchError::new(ErrorCode::CorruptArtifact, format!("Parquet 读取器构建失败：{e}")))?;

    let mut rows = Vec::new();
    for batch in reader {
        let batch = batch
            .map_err(|e| ResearchError::new(ErrorCode::CorruptArtifact, format!("Parquet 批次读取失败：{e}")))?;
        let n = batch.num_rows();
        let instruments = col::<StringArray>(&batch, 0)?;
        let dates = col::<Date32Array>(&batch, 1)?;
        let opens = col::<Decimal128Array>(&batch, 2)?;
        let highs = col::<Decimal128Array>(&batch, 3)?;
        let lows = col::<Decimal128Array>(&batch, 4)?;
        let closes = col::<Decimal128Array>(&batch, 5)?;
        let volumes = col::<Int64Array>(&batch, 6)?;
        let amounts = col::<Decimal128Array>(&batch, 7)?;
        for i in 0..n {
            rows.push(QuoteRow {
                instrument_id: instruments.value(i).to_string(),
                trade_date: date32_to_trade_date(dates.value(i)),
                open: Decimal::from_i128_with_scale(opens.value(i), PRICE_SCALE),
                high: Decimal::from_i128_with_scale(highs.value(i), PRICE_SCALE),
                low: Decimal::from_i128_with_scale(lows.value(i), PRICE_SCALE),
                close: Decimal::from_i128_with_scale(closes.value(i), PRICE_SCALE),
                volume_shares: volumes.value(i) as u64,
                amount_cny: if amounts.is_null(i) {
                    None
                } else {
                    Some(Decimal::from_i128_with_scale(amounts.value(i), PRICE_SCALE))
                },
            });
        }
    }
    rows.sort_by(|a, b| a.trade_date.cmp(&b.trade_date));
    Ok(rows)
}

fn col<'a, T: 'static>(batch: &'a RecordBatch, idx: usize) -> Result<&'a T> {
    batch
        .column(idx)
        .as_any()
        .downcast_ref::<T>()
        .ok_or_else(|| ResearchError::new(ErrorCode::CorruptArtifact, format!("分区列 {idx} 类型不符")))
}

fn map_io(context: &str, e: std::io::Error) -> ResearchError {
    if e.raw_os_error() == Some(28) {
        return ResearchError::new(ErrorCode::DiskFull, format!("{context}：磁盘空间不足"));
    }
    ResearchError::invalid(format!("{context}：{e}"))
}
