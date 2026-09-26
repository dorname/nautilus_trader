// -------------------------------------------------------------------------------------------------
//  Copyright (C) 2015-2026 Nautech Systems Pty Ltd. All rights reserved.
//  https://nautechsystems.io
//
//  Licensed under the GNU Lesser General Public License Version 3.0 (the "License");
//  You may not use this file except in compliance with the License.
//  You may obtain a copy of the License at https://www.gnu.org/licenses/lgpl-3.0.en.html
//
//  Unless required by applicable law or agreed to in writing, software
//  distributed under the License is distributed on an "AS IS" BASIS,
//  WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
//  See the License for the specific language governing permissions and
//  limitations under the License.
// -------------------------------------------------------------------------------------------------

// Links the workspace core from one shared library to collapse the binary's link time.
use std::{
    collections::HashSet,
    fs::{self, File},
    sync::{Arc, atomic::AtomicU64},
};

use datafusion::arrow::ipc::reader::StreamReader;
use nautilus_core::UnixNanos;
use nautilus_model::{
    data::{
        BookOrder, Data, FundingRateUpdate, OrderBookDelta, OrderBookDeltas, QuoteTick, TradeTick,
    },
    enums::{AggressorSide, BookAction, OrderSide},
    identifiers::{InstrumentId, TradeId},
    types::{Price, Quantity},
};
use nautilus_persistence::writer::feather::{FeatherWriter, RotationConfig, WriterClock};
use rstest::rstest;
use tempfile::TempDir;

#[rstest]
fn test_write_data_enum_quote() {
    let temp_dir = TempDir::new().unwrap();
    let clock = WriterClock::Test(Arc::new(AtomicU64::new(0)));

    let mut writer = FeatherWriter::new(
        temp_dir.path().to_path_buf(),
        clock,
        RotationConfig::NoRotation,
        None,
        None,
        None,
    );

    let quote = QuoteTick::new(
        InstrumentId::from("AUD/USD.SIM"),
        Price::from("1.0"),
        Price::from("1.0"),
        Quantity::from("1000"),
        Quantity::from("1000"),
        UnixNanos::from(1000),
        UnixNanos::from(1000),
    );

    writer.write_data(Data::Quote(quote)).unwrap();
    writer.close().unwrap();
}

#[rstest]
fn test_write_data_enum_all_types() {
    let temp_dir = TempDir::new().unwrap();
    let clock = WriterClock::Test(Arc::new(AtomicU64::new(0)));

    let mut writer = FeatherWriter::new(
        temp_dir.path().to_path_buf(),
        clock,
        RotationConfig::NoRotation,
        None,
        None,
        None,
    );

    let instrument_id = InstrumentId::from("AUD/USD.SIM");

    // Test all data types via write_data
    let quote = QuoteTick::new(
        instrument_id,
        Price::from("1.0"),
        Price::from("1.0"),
        Quantity::from("1000"),
        Quantity::from("1000"),
        UnixNanos::from(1000),
        UnixNanos::from(1000),
    );
    writer.write_data(Data::Quote(quote)).unwrap();

    let trade = TradeTick::new(
        instrument_id,
        Price::from("1.0"),
        Quantity::from("1000"),
        AggressorSide::Buy,
        TradeId::from("1"),
        UnixNanos::from(2000),
        UnixNanos::from(2000),
    );
    writer.write_data(Data::Trade(trade)).unwrap();

    let delta = OrderBookDelta::clear(
        instrument_id,
        0,
        UnixNanos::from(3000),
        UnixNanos::from(3000),
    );
    writer.write_data(Data::BookDelta(delta)).unwrap();

    let funding_rate = FundingRateUpdate::new(
        instrument_id,
        "0.0001".parse().unwrap(),
        Some(480),
        Some(UnixNanos::from(5_000)),
        UnixNanos::from(4_000),
        UnixNanos::from(4_000),
    );
    writer.write_data(Data::FundingRate(funding_rate)).unwrap();

    writer.close().unwrap();
}

#[rstest]
fn test_write_data_orderbook_deltas() {
    let temp_dir = TempDir::new().unwrap();
    let clock = WriterClock::Test(Arc::new(AtomicU64::new(0)));

    let mut writer = FeatherWriter::new(
        temp_dir.path().to_path_buf(),
        clock,
        RotationConfig::NoRotation,
        None,
        None,
        None,
    );

    let instrument_id = InstrumentId::from("AUD/USD.SIM");
    let delta1 = OrderBookDelta::clear(
        instrument_id,
        0,
        UnixNanos::from(1000),
        UnixNanos::from(1000),
    );
    let delta2 = OrderBookDelta::clear(
        instrument_id,
        0,
        UnixNanos::from(2000),
        UnixNanos::from(2000),
    );

    let deltas = OrderBookDeltas::new(instrument_id, vec![delta1, delta2]);
    // Test writing OrderBookDeltas via write_data
    writer
        .write_data(Data::BookDeltas(Box::new(deltas)))
        .unwrap();
    writer.close().unwrap();
}

#[rstest]
fn test_auto_flush() {
    let temp_dir = TempDir::new().unwrap();
    let shared_time = Arc::new(AtomicU64::new(0));
    let clock = WriterClock::Test(Arc::clone(&shared_time));

    let mut writer = FeatherWriter::new(
        temp_dir.path().to_path_buf(),
        clock,
        RotationConfig::NoRotation,
        None,
        None,
        Some(100), // 100ms flush interval
    );

    let quote = QuoteTick::new(
        InstrumentId::from("AUD/USD.SIM"),
        Price::from("1.0"),
        Price::from("1.0"),
        Quantity::from("1000"),
        Quantity::from("1000"),
        UnixNanos::from(1000),
        UnixNanos::from(1000),
    );

    // Write first quote; the interval has not elapsed so no bytes reach disk yet
    writer.write(quote).unwrap();
    let partial = temp_dir
        .path()
        .join("quotes")
        .join("quotes_0.feather.partial");
    assert_eq!(fs::metadata(&partial).unwrap().len(), 0);

    // Advance the shared time source past the 100ms flush interval
    shared_time.store(200_000_000, std::sync::atomic::Ordering::Relaxed);

    // Second write hits the auto-flush boundary and appends both quotes to the same file
    let quote2 = QuoteTick::new(
        InstrumentId::from("AUD/USD.SIM"),
        Price::from("1.1"),
        Price::from("1.1"),
        Quantity::from("1000"),
        Quantity::from("1000"),
        UnixNanos::from(2000),
        UnixNanos::from(2000),
    );
    writer.write(quote2).unwrap();

    let rows = StreamReader::try_new(File::open(&partial).unwrap(), None)
        .unwrap()
        .map(|batch| batch.unwrap().num_rows())
        .sum::<usize>();
    assert_eq!(rows, 2);
    assert_eq!(temp_dir.path().read_dir().unwrap().count(), 1);
}

#[rstest]
fn test_close() {
    let temp_dir = TempDir::new().unwrap();
    let clock = WriterClock::Test(Arc::new(AtomicU64::new(0)));

    let mut writer = FeatherWriter::new(
        temp_dir.path().to_path_buf(),
        clock,
        RotationConfig::NoRotation,
        None,
        None,
        None,
    );

    let quote = QuoteTick::new(
        InstrumentId::from("AUD/USD.SIM"),
        Price::from("1.0"),
        Price::from("1.0"),
        Quantity::from("1000"),
        Quantity::from("1000"),
        UnixNanos::from(1000),
        UnixNanos::from(1000),
    );

    writer.write(quote).unwrap();

    // Close should seal and clear writers
    writer.close().unwrap();
}

// Note: Message bus subscription test is skipped due to async/sync boundary complexity.
// The handler uses block_on which can't be used from within an async runtime (tokio test).
// This functionality is better tested via Python integration tests where the message bus
// is used in a non-async context or via proper async task spawning.

// Regression test for https://github.com/nautechsystems/nautilus_trader/issues/3913,
// where a leading BookAction::Clear delta poisoned file metadata with 0 precision.
#[rstest]
fn test_write_orderbook_deltas_clear_first_preserves_precision() {
    let temp_dir = TempDir::new().unwrap();
    let clock = WriterClock::Test(Arc::new(AtomicU64::new(0)));

    let mut per_instrument = HashSet::new();
    per_instrument.insert("order_book_deltas".to_string());

    let mut writer = FeatherWriter::new(
        temp_dir.path().to_path_buf(),
        clock,
        RotationConfig::NoRotation,
        None,
        Some(per_instrument),
        None,
    );

    let instrument_id = InstrumentId::from("AUD/USD.SIM");
    let clear = OrderBookDelta::clear(
        instrument_id,
        0,
        UnixNanos::from(1000),
        UnixNanos::from(1000),
    );

    let add = OrderBookDelta::new(
        instrument_id,
        BookAction::Add,
        BookOrder {
            side: OrderSide::Buy.into(),
            price: Price::new(1.23, 2),
            size: Quantity::new(100.0, 6),
            order_id: 1,
        },
        0,
        1,
        UnixNanos::from(2000),
        UnixNanos::from(2000),
    );

    let deltas = OrderBookDeltas::new(instrument_id, vec![clear, add]);
    writer
        .write_data(Data::BookDeltas(Box::new(deltas)))
        .unwrap();
    writer.close().unwrap();

    let feather_path = find_feather_file(temp_dir.path());
    let file = File::open(&feather_path).unwrap();
    let reader = StreamReader::try_new(file, None).unwrap();
    let metadata = reader.schema().metadata().clone();

    assert_eq!(
        metadata.get("price_precision"),
        Some(&"2".to_string()),
        "file metadata should reflect real price precision, not the CLEAR sentinel",
    );
    assert_eq!(
        metadata.get("size_precision"),
        Some(&"6".to_string()),
        "file metadata should reflect real size precision, not the CLEAR sentinel",
    );
}

// Regression test for the all-sentinel fallback: a batch containing only
// BookAction::Clear rows has no real precision to derive, so file metadata
// legitimately carries price_precision=0, size_precision=0.
#[rstest]
fn test_write_orderbook_deltas_all_sentinel_metadata_fallback() {
    let temp_dir = TempDir::new().unwrap();
    let clock = WriterClock::Test(Arc::new(AtomicU64::new(0)));

    let mut per_instrument = HashSet::new();
    per_instrument.insert("order_book_deltas".to_string());

    let mut writer = FeatherWriter::new(
        temp_dir.path().to_path_buf(),
        clock,
        RotationConfig::NoRotation,
        None,
        Some(per_instrument),
        None,
    );

    let instrument_id = InstrumentId::from("AUD/USD.SIM");
    let clear1 = OrderBookDelta::clear(
        instrument_id,
        0,
        UnixNanos::from(1000),
        UnixNanos::from(1000),
    );
    let clear2 = OrderBookDelta::clear(
        instrument_id,
        1,
        UnixNanos::from(2000),
        UnixNanos::from(2000),
    );

    let deltas = OrderBookDeltas::new(instrument_id, vec![clear1, clear2]);
    writer
        .write_data(Data::BookDeltas(Box::new(deltas)))
        .unwrap();
    writer.close().unwrap();

    let feather_path = find_feather_file(temp_dir.path());
    let file = File::open(&feather_path).unwrap();
    let reader = StreamReader::try_new(file, None).unwrap();
    let metadata = reader.schema().metadata().clone();

    assert_eq!(metadata.get("price_precision"), Some(&"0".to_string()));
    assert_eq!(metadata.get("size_precision"), Some(&"0".to_string()));
}

// Regression test for the mixed-instrument routing in write_batch. When a
// batch contains deltas for multiple instruments, each instrument's rows
// must land in its own file with its own precision metadata.
#[rstest]
fn test_write_batch_partitions_by_instrument() {
    let temp_dir = TempDir::new().unwrap();
    let clock = WriterClock::Test(Arc::new(AtomicU64::new(0)));

    let mut per_instrument = HashSet::new();
    per_instrument.insert("order_book_deltas".to_string());

    let mut writer = FeatherWriter::new(
        temp_dir.path().to_path_buf(),
        clock,
        RotationConfig::NoRotation,
        None,
        Some(per_instrument),
        None,
    );

    let instrument_a = InstrumentId::from("AUD/USD.SIM");
    let instrument_b = InstrumentId::from("BTC/USD.BINANCE");

    let make_add = |instrument_id, price: f64, price_prec, size: f64, size_prec, ts| {
        OrderBookDelta::new(
            instrument_id,
            BookAction::Add,
            BookOrder {
                side: OrderSide::Buy.into(),
                price: Price::new(price, price_prec),
                size: Quantity::new(size, size_prec),
                order_id: 1,
            },
            0,
            1,
            UnixNanos::from(ts),
            UnixNanos::from(ts),
        )
    };

    let deltas = vec![
        make_add(instrument_a, 1.23, 2, 100.0, 0, 1000),
        make_add(instrument_b, 20_000.0, 4, 0.123_456_78, 8, 2000),
        make_add(instrument_a, 1.24, 2, 50.0, 0, 3000),
        make_add(instrument_b, 20_100.0, 4, 0.25, 8, 4000),
    ];

    writer.write_batch(deltas).unwrap();
    writer.close().unwrap();

    let files = collect_feather_files(temp_dir.path());
    assert_eq!(
        files.len(),
        2,
        "expected one file per instrument, found {files:?}"
    );

    let mut by_instrument = std::collections::HashMap::new();

    for path in files {
        let reader = StreamReader::try_new(File::open(&path).unwrap(), None).unwrap();
        let metadata = reader.schema().metadata().clone();
        let instrument_id = metadata
            .get("instrument_id")
            .expect("instrument_id metadata")
            .clone();
        by_instrument.insert(instrument_id, metadata);
    }

    let metadata_a = by_instrument.get("AUD/USD.SIM").expect("AUD/USD.SIM file");
    assert_eq!(metadata_a.get("price_precision"), Some(&"2".to_string()));
    assert_eq!(metadata_a.get("size_precision"), Some(&"0".to_string()));

    let metadata_b = by_instrument
        .get("BTC/USD.BINANCE")
        .expect("BTC/USD.BINANCE file");
    assert_eq!(metadata_b.get("price_precision"), Some(&"4".to_string()));
    assert_eq!(metadata_b.get("size_precision"), Some(&"8".to_string()));
}

fn collect_feather_files(dir: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    collect_feather_files_into(dir, &mut out);
    out
}

fn collect_feather_files_into(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            collect_feather_files_into(&path, out);
        } else if path.extension().and_then(|s| s.to_str()) == Some("feather") {
            out.push(path);
        }
    }
}

fn find_feather_file(dir: &std::path::Path) -> std::path::PathBuf {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            let found = find_feather_file(&path);
            if !found.as_os_str().is_empty() {
                return found;
            }
        } else if path.extension().and_then(|s| s.to_str()) == Some("feather") {
            return path;
        }
    }

    std::path::PathBuf::new()
}
