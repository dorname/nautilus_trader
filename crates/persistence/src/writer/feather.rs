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

#![expect(
    clippy::missing_errors_doc,
    reason = "Feather writer public methods forward encoding and file IO errors directly"
)]

use std::{
    any::Any,
    cell::RefCell,
    collections::{BTreeMap, HashMap, HashSet},
    fmt::Debug,
    fs::{self, File, OpenOptions},
    io::{self, BufWriter, Write},
    path::{Path, PathBuf},
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};

use ahash::AHashMap;
use datafusion::arrow::{
    array::StringArray,
    datatypes::{DataType, Field, Schema},
    error::ArrowError,
    ipc::writer::StreamWriter,
    record_batch::RecordBatch,
};
use jiff::{
    SignedDuration,
    civil::Time,
    tz::{AmbiguousOffset, TimeZone},
};
use nautilus_common::{clock::Clock, live::LiveClock};
use nautilus_core::{DurationNanos, UnixNanos, time::nanos_since_unix_epoch};
use nautilus_model::{
    data::{
        Bar, CustomData, CustomDataTrait, Data, DataBatch, FundingRateUpdate, IndexPriceUpdate,
        InstrumentStatus, MarkPriceUpdate, OptionGreeks, OrderBookDelta, OrderBookDeltas,
        OrderBookDepth, QuoteTick, TradeTick, close::InstrumentClose, encode_custom_to_arrow,
        get_arrow_schema,
    },
    events::{
        AccountState, OrderAccepted, OrderCancelRejected, OrderCanceled, OrderDenied,
        OrderEmulated, OrderEventAny, OrderExpired, OrderFillVoided, OrderFilled, OrderInitialized,
        OrderModifyRejected, OrderPendingCancel, OrderPendingUpdate, OrderRejected, OrderReleased,
        OrderSnapshot, OrderSubmitted, OrderTriggered, OrderUpdated, PositionAdjusted,
        PositionChanged, PositionClosed, PositionEvent, PositionOpened, PositionSnapshot,
    },
    instruments::InstrumentAny,
    reports::{ExecutionMassStatus, FillReport, OrderStatusReport, PositionStatusReport},
};
use nautilus_serialization::arrow::{
    EncodeToRecordBatch, KEY_INSTRUMENT_ID, catalog_identifier_from_metadata,
    record_batch_with_identifier_column, schema_with_identifier_column,
};

use crate::{
    common::{
        custom::{
            augment_batch_with_data_type_column, schema_with_data_type_column,
            validate_custom_catalog_schema,
        },
        paths::{CatalogPathPrefix, urisafe_instrument_id},
    },
    writer::{
        filter::WriterRecordFilter,
        subscription::StreamingSinkSubscription,
        traits::{StreamingDataSink, StreamingSink},
    },
};

pub(crate) type FeatherWriteCommand =
    Box<dyn FnOnce(&mut FeatherWriter) -> Result<(), Box<dyn std::error::Error>> + Send + 'static>;

#[expect(
    clippy::needless_pass_by_value,
    reason = "map_err transfers ownership of the boxed writer error"
)]
pub(crate) fn feather_error(e: Box<dyn std::error::Error>) -> anyhow::Error {
    anyhow::anyhow!("{e}")
}

macro_rules! define_builtin_data_batch_dispatch {
    ($(($variant:ident, $type:ident, $data:ident, $batch:ident, $prefix:literal)),+ $(,)?) => {
        fn write_builtin_data_batch(
            writer: &mut FeatherWriter,
            batch: &DataBatch,
        ) -> Option<Result<(), Box<dyn std::error::Error>>> {
            match batch {
                $(
                    DataBatch::$batch(data) => Some(writer.write_batch(data.as_ref().to_vec())),
                )+
                _ => None,
            }
        }
    };
}

nautilus_model::for_each_data_type!(define_builtin_data_batch_dispatch);

pub(crate) const NAUTILUS_ARROW_METADATA_ID_COLUMN: &str = "nautilus_metadata_id";
pub(crate) const NAUTILUS_ARROW_METADATA_JSON_COLUMN: &str = "nautilus_metadata_json";

#[derive(Clone, Debug, Eq, PartialEq)]
struct StagedArrowMetadataRow {
    metadata_id: String,
    metadata_json: String,
}

#[derive(Debug, Default, PartialEq, PartialOrd, Hash, Eq, Clone)]
pub struct FileWriterPath {
    path: PathBuf,
    type_str: String,
    instrument_id: Option<String>,
}

/// Minimal `Send` time source for streaming writers.
///
/// `Live` reads the wall clock directly; `Test` reads a shared atomic that the
/// owner of the source clock (e.g. a PyO3 wrapper holding a `VirtualClock`)
/// refreshes before forwarding writer calls.
#[derive(Clone, Debug)]
pub enum WriterClock {
    /// Wall-clock time via [`nanos_since_unix_epoch`].
    Live,
    /// Externally driven time stored in a shared atomic (nanoseconds since the UNIX epoch).
    Test(Arc<AtomicU64>),
}

impl WriterClock {
    /// Returns the current timestamp in nanoseconds since the UNIX epoch.
    #[must_use]
    pub fn timestamp_ns(&self) -> UnixNanos {
        match self {
            Self::Live => UnixNanos::from(nanos_since_unix_epoch()),
            Self::Test(shared) => UnixNanos::from(shared.load(Ordering::Relaxed)),
        }
    }

    /// Builds a writer clock from a shared `dyn Clock`.
    ///
    /// Returns [`WriterClock::Live`] for a [`LiveClock`]. For any other clock
    /// (e.g. `VirtualClock`) it returns a [`WriterClock::Test`] source seeded with
    /// the clock's current time, plus the shared atomic the caller must refresh
    /// from the source clock before each forwarded writer call.
    #[must_use]
    pub fn from_shared_clock(clock: &Rc<RefCell<dyn Clock>>) -> (Self, Option<Arc<AtomicU64>>) {
        let borrowed = clock.borrow();
        let any_ref: &dyn Any = &*borrowed;
        if any_ref.downcast_ref::<LiveClock>().is_some() {
            (Self::Live, None)
        } else {
            let shared = Arc::new(AtomicU64::new(borrowed.timestamp_ns().as_u64()));
            (Self::Test(Arc::clone(&shared)), Some(shared))
        }
    }
}

fn record_batch_with_delta_staged_metadata(
    batch: &RecordBatch,
    metadata_rows: &[StagedArrowMetadataRow],
) -> anyhow::Result<RecordBatch> {
    anyhow::ensure!(
        batch.num_rows() == metadata_rows.len(),
        "Delta Feather staging metadata row count {} does not match batch row count {}",
        metadata_rows.len(),
        batch.num_rows()
    );
    anyhow::ensure!(
        batch
            .schema()
            .index_of(NAUTILUS_ARROW_METADATA_ID_COLUMN)
            .is_err(),
        "Delta Feather staging schema already has {NAUTILUS_ARROW_METADATA_ID_COLUMN}"
    );
    anyhow::ensure!(
        batch
            .schema()
            .index_of(NAUTILUS_ARROW_METADATA_JSON_COLUMN)
            .is_err(),
        "Delta Feather staging schema already has {NAUTILUS_ARROW_METADATA_JSON_COLUMN}"
    );

    let mut fields = batch
        .schema()
        .fields()
        .iter()
        .map(|field| {
            Arc::new(Field::new(
                field.name().clone(),
                field.data_type().clone(),
                field.is_nullable(),
            ))
        })
        .collect::<Vec<_>>();

    fields.push(Arc::new(Field::new(
        NAUTILUS_ARROW_METADATA_ID_COLUMN,
        DataType::Utf8,
        false,
    )));
    fields.push(Arc::new(Field::new(
        NAUTILUS_ARROW_METADATA_JSON_COLUMN,
        DataType::Utf8,
        false,
    )));

    let mut columns = batch.columns().to_vec();
    columns.push(Arc::new(StringArray::from(
        metadata_rows
            .iter()
            .map(|row| row.metadata_id.clone())
            .collect::<Vec<_>>(),
    )));
    columns.push(Arc::new(StringArray::from(
        metadata_rows
            .iter()
            .map(|row| row.metadata_json.clone())
            .collect::<Vec<_>>(),
    )));

    Ok(RecordBatch::try_new(
        Arc::new(Schema::new(fields)),
        columns,
    )?)
}

fn arrow_metadata_row(
    metadata: &HashMap<String, String>,
    fields: &arrow::datatypes::Fields,
) -> anyhow::Result<StagedArrowMetadataRow> {
    let schema_metadata = metadata
        .iter()
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect::<BTreeMap<_, _>>();

    let field_metadata = fields
        .iter()
        .filter(|field| !field.metadata().is_empty())
        .map(|field| {
            (
                field.name().clone(),
                field
                    .metadata()
                    .iter()
                    .map(|(key, value)| (key.clone(), value.clone()))
                    .collect::<BTreeMap<_, _>>(),
            )
        })
        .collect::<BTreeMap<_, _>>();

    let metadata_json = serde_json::to_string(&serde_json::json!({
        "format_version": 1,
        "schema_metadata": schema_metadata,
        "field_metadata": field_metadata,
    }))?;
    let metadata_id = staged_metadata_id(&canonical_metadata_json(metadata)?);

    Ok(StagedArrowMetadataRow {
        metadata_id,
        metadata_json,
    })
}

/// Canonical schema-level metadata JSON: keys serialized in sorted order.
///
/// The staged metadata id hashes this exact string so readers can verify the schema metadata
/// independently of its storage location.
pub(crate) fn canonical_metadata_json(
    metadata: &HashMap<String, String>,
) -> anyhow::Result<String> {
    Ok(serde_json::to_string(
        &metadata
            .iter()
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect::<BTreeMap<_, _>>(),
    )?)
}

pub(crate) fn staged_metadata_id(metadata_json: &str) -> String {
    format!("blake3:{}", blake3::hash(metadata_json.as_bytes()).to_hex())
}

/// File extension of a sealed Feather stream file.
pub(crate) const FEATHER_EXTENSION: &str = "feather";

/// File extension of a Feather stream file that is still being appended to.
pub(crate) const FEATHER_PARTIAL_EXTENSION: &str = "feather.partial";

/// An open local Feather file that appends Arrow IPC stream batches.
///
/// Batches go to `{name}.feather.partial`, which is renamed to `{name}.feather` when the file is
/// sealed, so readers listing `.feather` files only see complete streams.
struct FeatherFile {
    writer: StreamWriter<CountingWriter<BufWriter<File>>>,
    partial_path: PathBuf,
    path: PathBuf,
    schema: Schema,
    max_size: Option<u64>,
}

impl FeatherFile {
    fn create(
        path: &Path,
        schema: &Schema,
        rotation_config: &RotationConfig,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let partial_path = path.with_extension(FEATHER_PARTIAL_EXTENSION);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&partial_path)?;
        let writer = StreamWriter::try_new(CountingWriter::new(BufWriter::new(file)), schema)?;
        let max_size = match rotation_config {
            RotationConfig::Size { max_size } => Some(*max_size),
            _ => None,
        };

        Ok(Self {
            writer,
            partial_path,
            path: path.to_path_buf(),
            schema: schema.clone(),
            max_size,
        })
    }

    // Returns true when size rotation is due
    fn write_record_batch(&mut self, batch: &RecordBatch) -> Result<bool, ArrowError> {
        let batch = if batch.schema().as_ref() == &self.schema {
            batch.clone()
        } else {
            RecordBatch::try_new(Arc::new(self.schema.clone()), batch.columns().to_vec())?
        };

        self.writer.write(&batch)?;
        Ok(self
            .max_size
            .is_some_and(|max_size| self.size() >= max_size))
    }

    fn size(&self) -> u64 {
        self.writer.get_ref().bytes
    }

    fn flush(&mut self) -> Result<(), ArrowError> {
        self.writer.flush()
    }

    fn seal(mut self) -> Result<(), Box<dyn std::error::Error>> {
        self.writer.finish()?;
        let file = self
            .writer
            .into_inner()?
            .inner
            .into_inner()
            .map_err(io::IntoInnerError::into_error)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&self.partial_path, &self.path)?;
        Ok(())
    }
}

/// Counts the bytes written through the inner writer.
struct CountingWriter<W> {
    inner: W,
    bytes: u64,
}

impl<W> CountingWriter<W> {
    const fn new(inner: W) -> Self {
        Self { inner, bytes: 0 }
    }
}

impl<W: Write> Write for CountingWriter<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let written = self.inner.write(buf)?;
        self.bytes += written as u64;
        Ok(written)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

/// Rotations and flushes collected by the synchronous encode path and run once it completes.
#[derive(Debug, Default)]
struct PendingIo {
    rotate_paths: Vec<FileWriterPath>,
    flush_due: bool,
}

/// Configuration for file rotation.
#[derive(Debug, Clone)]
pub enum RotationConfig {
    /// Rotate based on file size.
    Size {
        /// Maximum buffer size in bytes before rotation.
        max_size: u64,
    },
    /// Rotate based on a time interval.
    Interval {
        /// Interval in nanoseconds.
        interval_ns: u64,
    },
    /// Rotate based on scheduled dates.
    ScheduledDates {
        /// Interval in nanoseconds.
        interval_ns: u64,
        /// Time of day for rotation (nanoseconds since midnight).
        rotation_time: UnixNanos,
        /// Timezone for rotation calculations.
        rotation_timezone: TimeZone,
    },
    /// No automatic rotation.
    NoRotation,
}

impl RotationConfig {
    /// Creates scheduled rotation using UTC.
    ///
    /// This keeps timezone ownership in writer backend when callers expose
    /// only a time-of-day schedule without a timezone field.
    #[must_use]
    pub const fn scheduled_utc(interval_ns: u64, rotation_time: UnixNanos) -> Self {
        Self::ScheduledDates {
            interval_ns,
            rotation_time,
            rotation_timezone: jiff::tz::TimeZone::UTC,
        }
    }
}

/// Streams encoded data into one local Feather file per record type and identifier.
///
/// The `write()` method is the single entry point for clients: they supply a data value (of generic type T)
/// and the manager encodes it (using T's metadata via `EncodeToRecordBatch`), routes it by `CatalogPathPrefix`,
/// and appends it to that key's open file. Flushing pushes buffered bytes to disk without starting
/// a new file; a file is sealed and replaced only on rotation, [`Self::seal`], or [`Self::close`].
pub struct FeatherWriter {
    /// Local directory for writing files.
    directory: PathBuf,
    /// Send time source for timestamps, rotation, and flush cadence.
    clock: WriterClock,
    /// Rotation configuration.
    rotation_config: RotationConfig,
    /// Optional set of type names to include.
    included_types: Option<HashSet<String>>,
    /// Optional typed record-family filter.
    record_filter: Option<WriterRecordFilter>,
    /// Set of types that should be split by instrument.
    per_instrument_types: HashSet<String>,
    /// Open files keyed by their sealed path.
    writers: HashMap<FileWriterPath, FeatherFile>,
    /// Paths already handed out by this writer instance.
    reserved_paths: HashSet<PathBuf>,
    /// Map of next rotation times keyed by their path.
    next_rotation_times: HashMap<FileWriterPath, UnixNanos>,
    /// Flush interval in milliseconds (0 = no automatic flushing).
    flush_interval_ms: u64,
    /// Last flush timestamp in nanoseconds.
    last_flush_ns: UnixNanos,
    /// Whether staged batches include the catalog row identifier column.
    catalog_identifier_column: bool,
    pending_write_error: Option<String>,
}

impl FeatherWriter {
    /// Creates a new [`FeatherWriter`] instance.
    #[must_use]
    pub fn new(
        directory: PathBuf,
        clock: WriterClock,
        rotation_config: RotationConfig,
        included_types: Option<HashSet<String>>,
        per_instrument_types: Option<HashSet<String>>,
        flush_interval_ms: Option<u64>,
    ) -> Self {
        let flush_interval_ms = flush_interval_ms.unwrap_or(1000); // Default 1 second
        let last_flush_ns = clock.timestamp_ns();

        Self {
            directory,
            clock,
            rotation_config,
            included_types,
            record_filter: None,
            per_instrument_types: per_instrument_types.unwrap_or_default(),
            writers: HashMap::new(),
            reserved_paths: HashSet::new(),
            next_rotation_times: HashMap::new(),
            flush_interval_ms,
            last_flush_ns,
            catalog_identifier_column: false,
            pending_write_error: None,
        }
    }

    pub(crate) fn default_per_instrument_types() -> HashSet<String> {
        HashSet::from([
            Bar::path_prefix().to_string(),
            OrderBookDelta::path_prefix().to_string(),
            OrderBookDepth::path_prefix().to_string(),
            OptionGreeks::path_prefix().to_string(),
            QuoteTick::path_prefix().to_string(),
            TradeTick::path_prefix().to_string(),
            MarkPriceUpdate::path_prefix().to_string(),
            IndexPriceUpdate::path_prefix().to_string(),
            FundingRateUpdate::path_prefix().to_string(),
        ])
    }

    /// Sets typed record-family filter for subsequent writes.
    #[must_use]
    pub fn with_record_filter(mut self, record_filter: Option<WriterRecordFilter>) -> Self {
        self.record_filter = record_filter;
        self
    }

    /// Includes the catalog row identifier column in staged batches.
    #[must_use]
    pub fn with_catalog_identifier_column(mut self) -> Self {
        self.catalog_identifier_column = true;
        self
    }

    /// Writes a single data value.
    ///
    /// This is the user entry point. The data is encoded into a `RecordBatch` and appended to the
    /// open file for its type and identifier.
    pub fn write<T>(&mut self, data: T) -> Result<(), Box<dyn std::error::Error>>
    where
        T: EncodeToRecordBatch + CatalogPathPrefix + 'static,
    {
        let metadata = T::metadata(&data);
        let identifier = catalog_identifier_from_metadata(&metadata);

        let instrument_type = if T::path_prefix() == InstrumentAny::path_prefix() {
            metadata.get("class").map(String::as_str)
        } else {
            None
        };

        if !self.should_write_record(T::path_prefix(), identifier.as_deref(), instrument_type) {
            return Ok(());
        }

        let path = self.get_writer_path(&data)?;

        // Create a new FileWriter if one does not exist.
        if !self.writers.contains_key(&path) {
            self.create_writer::<T>(path.clone(), &data)?;
        }

        // Encode the data into a RecordBatch using T's encoding logic.
        let mut batch = T::encode_batch(&metadata, &[data])?;
        let stage_delta_metadata =
            self.catalog_identifier_column && T::path_prefix() != InstrumentAny::path_prefix();

        if stage_delta_metadata {
            let metadata_row = arrow_metadata_row(&metadata, batch.schema().fields())?;
            batch = record_batch_with_identifier_column(batch, identifier.as_deref())?;
            batch = record_batch_with_delta_staged_metadata(
                &batch,
                std::slice::from_ref(&metadata_row),
            )?;
        }

        // Write the RecordBatch to the appropriate FileWriter.
        let mut pending = PendingIo::default();

        self.stage_batch_write(path, &batch, &mut pending)?;
        pending.flush_due = self.flush_is_due();

        self.complete_pending_io(&pending)
    }

    /// Writes a batch of data values as one or more `RecordBatch`es.
    ///
    /// Uses `T::chunk_metadata` to derive the file schema metadata. This protects
    /// types like `OrderBookDelta` from having their file metadata poisoned by a
    /// leading sentinel row (e.g. `BookAction::Clear`, which carries
    /// `price_precision=0, size_precision=0`).
    ///
    /// Per-instrument types are partitioned by instrument so a mixed-instrument
    /// batch lands in the correct file for each instrument.
    pub fn write_batch<T>(&mut self, data: Vec<T>) -> Result<(), Box<dyn std::error::Error>>
    where
        T: EncodeToRecordBatch + CatalogPathPrefix + 'static,
    {
        if data.is_empty() || !self.should_write_type::<T>() {
            return Ok(());
        }

        // Group by logical writer identity. Catalog staging opts into
        // `catalog_identifier_column`, so row identifiers remain in the batch and
        // mixed-identifier input can share the type-level Feather file path.
        // Grouping on FileWriterPath would split same-instrument rows across distinct
        // timestamped paths when the writer does not yet exist under a LiveClock.
        let type_str = T::path_prefix();
        let needs_instrument = type_str == InstrumentAny::path_prefix()
            || self.per_instrument_types.contains(type_str)
            || type_str.starts_with("custom_");

        let mut groups: AHashMap<Option<String>, Vec<T>> = AHashMap::new();

        for item in data {
            let metadata = T::metadata(&item);
            let identifier = catalog_identifier_from_metadata(&metadata);

            let instrument_type = if type_str == InstrumentAny::path_prefix() {
                metadata.get("class").map(String::as_str)
            } else {
                None
            };

            if !self.should_write_record(type_str, identifier.as_deref(), instrument_type) {
                continue;
            }

            let group_identifier = if self.catalog_identifier_column
                && type_str != InstrumentAny::path_prefix()
            {
                identifier.clone()
            } else if type_str == InstrumentAny::path_prefix() || !self.catalog_identifier_column {
                needs_instrument
                        .then(|| {
                            metadata.get(KEY_INSTRUMENT_ID).cloned().ok_or_else(|| {
                                format!(
                                    "Data {type_str} expected instrument_id metadata for per instrument writer"
                                )
                            })
                        })
                        .transpose()?
            } else {
                None
            };

            groups.entry(group_identifier).or_default().push(item);
        }

        if groups.is_empty() {
            return Ok(());
        }

        let mut pending = PendingIo::default();

        for group in groups.into_values() {
            let path = self.get_writer_path(&group[0])?;
            let metadata = T::chunk_metadata(&group);

            if !self.writers.contains_key(&path) {
                self.create_writer_with_metadata::<T>(path.clone(), metadata.clone())?;
            }

            let identifier = catalog_identifier_from_metadata(&metadata);
            let stage_delta_metadata =
                self.catalog_identifier_column && type_str != InstrumentAny::path_prefix();
            let mut batch = T::encode_batch(&metadata, &group)?;

            let metadata_rows = if stage_delta_metadata {
                group
                    .iter()
                    .map(T::metadata)
                    .map(|metadata| arrow_metadata_row(&metadata, batch.schema().fields()))
                    .collect::<anyhow::Result<Vec<_>>>()?
            } else {
                Vec::new()
            };

            if stage_delta_metadata {
                batch = record_batch_with_identifier_column(batch, identifier.as_deref())?;
                batch = record_batch_with_delta_staged_metadata(&batch, &metadata_rows)?;
            }

            self.stage_batch_write(path, &batch, &mut pending)?;
        }

        pending.flush_due = self.flush_is_due();

        self.complete_pending_io(&pending)
    }

    fn stage_batch_write(
        &mut self,
        path: FileWriterPath,
        batch: &RecordBatch,
        pending: &mut PendingIo,
    ) -> Result<(), Box<dyn std::error::Error>> {
        if let Some(writer) = self.writers.get_mut(&path) {
            let should_rotate = writer.write_record_batch(batch)?;
            if should_rotate || self.check_scheduled_rotation(&path) {
                pending.rotate_paths.push(path);
            }
        }

        Ok(())
    }

    /// Returns whether the auto-flush interval has elapsed since the last flush.
    fn flush_is_due(&self) -> bool {
        if self.flush_interval_ms == 0 {
            return false; // Auto-flush disabled
        }

        let now_ns = self.clock.timestamp_ns();
        let elapsed_ms = now_ns.as_u64().saturating_sub(self.last_flush_ns.as_u64()) / 1_000_000;
        elapsed_ms >= self.flush_interval_ms
    }

    fn complete_pending_io(
        &mut self,
        pending: &PendingIo,
    ) -> Result<(), Box<dyn std::error::Error>> {
        for path in &pending.rotate_paths {
            self.seal_file(path)?;
        }

        if pending.flush_due {
            self.flush()?;
        }

        Ok(())
    }

    fn check_scheduled_rotation(&mut self, path: &FileWriterPath) -> bool {
        match &self.rotation_config {
            RotationConfig::Interval { interval_ns } => {
                let now = self.clock.timestamp_ns();
                let next_rotation = self.next_rotation_times.get(path).copied();

                match next_rotation {
                    None => {
                        self.next_rotation_times
                            .insert(path.clone(), now + DurationNanos::new(*interval_ns));
                        false
                    }
                    Some(next) if now >= next => {
                        self.next_rotation_times
                            .insert(path.clone(), now + DurationNanos::new(*interval_ns));
                        true
                    }
                    _ => false,
                }
            }
            RotationConfig::ScheduledDates {
                interval_ns,
                rotation_time,
                rotation_timezone,
            } => {
                let now = self.clock.timestamp_ns();
                let next_rotation = self.next_rotation_times.get(path).copied();

                match next_rotation {
                    None => {
                        let next = self.calculate_next_scheduled_rotation(
                            *rotation_time,
                            rotation_timezone,
                            *interval_ns,
                        );
                        self.next_rotation_times.insert(path.clone(), next);
                        false
                    }
                    Some(next) if now >= next => {
                        let next = self.calculate_next_scheduled_rotation(
                            *rotation_time,
                            rotation_timezone,
                            *interval_ns,
                        );
                        self.next_rotation_times.insert(path.clone(), next);
                        true
                    }
                    _ => false,
                }
            }
            _ => false,
        }
    }

    fn calculate_next_scheduled_rotation(
        &self,
        rotation_time: UnixNanos,
        rotation_timezone: &TimeZone,
        interval_ns: u64,
    ) -> UnixNanos {
        let now_utc = self.clock.timestamp_ns().to_datetime_utc();
        let now_local = rotation_timezone.to_datetime(now_utc);

        let rotation_time_secs = u32::try_from(*rotation_time / 1_000_000_000).unwrap_or(0);
        let rotation_time_nanos = i32::try_from(*rotation_time % 1_000_000_000).unwrap_or(0);

        let rotation_time = if rotation_time_secs < 86_400 {
            Time::new(
                i8::try_from(rotation_time_secs / 3_600).unwrap_or(0),
                i8::try_from(rotation_time_secs % 3_600 / 60).unwrap_or(0),
                i8::try_from(rotation_time_secs % 60).unwrap_or(0),
                rotation_time_nanos,
            )
            .unwrap_or(Time::MIN)
        } else {
            Time::MIN
        };

        let local_rotation = now_local.date().to_datetime(rotation_time);
        let ambiguous = rotation_timezone.to_ambiguous_timestamp(local_rotation);

        let mut next_rotation = match ambiguous.offset() {
            AmbiguousOffset::Gap { .. } => now_utc,
            _ => ambiguous.earlier().unwrap_or(now_utc),
        };

        if next_rotation <= now_utc {
            // If the time has already passed today, we would usually add the interval
            // But let's align exactly with how Python does it:
            while next_rotation <= now_utc {
                next_rotation += SignedDuration::from_nanos_i128(i128::from(interval_ns));
            }
        }

        UnixNanos::from(u64::try_from(next_rotation.as_nanosecond()).unwrap_or(0))
    }

    // Seals the open file at `path`; the next write for its key opens a new file
    fn seal_file(&mut self, path: &FileWriterPath) -> Result<(), Box<dyn std::error::Error>> {
        self.next_rotation_times.remove(path);
        match self.writers.remove(path) {
            Some(file) => file.seal(),
            None => Ok(()),
        }
    }

    /// Creates (and inserts) a new `FileWriter` for type T.
    fn create_writer<T>(
        &mut self,
        path: FileWriterPath,
        data: &T,
    ) -> Result<(), Box<dyn std::error::Error>>
    where
        T: EncodeToRecordBatch + CatalogPathPrefix + 'static,
    {
        self.create_writer_with_metadata::<T>(path, T::metadata(data))
    }

    /// Creates (and inserts) a new `FileWriter` for type T with pre-computed metadata.
    ///
    /// Use this variant when the caller has selected metadata from a chunk
    /// (e.g. via `T::chunk_metadata`) to avoid schema poisoning by sentinel rows.
    fn create_writer_with_metadata<T>(
        &mut self,
        path: FileWriterPath,
        metadata: HashMap<String, String>,
    ) -> Result<(), Box<dyn std::error::Error>>
    where
        T: EncodeToRecordBatch + CatalogPathPrefix + 'static,
    {
        let type_str = T::path_prefix();
        let stage_delta_metadata =
            self.catalog_identifier_column && type_str != InstrumentAny::path_prefix();

        let schema = if self.catalog_identifier_column
            || type_str == InstrumentAny::path_prefix()
            || self.per_instrument_types.contains(type_str)
        {
            T::get_schema(Some(metadata))
        } else {
            T::get_schema(None)
        };

        let schema = if stage_delta_metadata {
            Self::schema_with_delta_staging_columns(&schema_with_identifier_column(&schema))
        } else {
            schema
        };

        let file = FeatherFile::create(&path.path, &schema, &self.rotation_config)?;
        self.writers.insert(path, file);
        Ok(())
    }

    /// Creates (and inserts) a new `FeatherBuffer` for custom data at the given path.
    fn create_custom_writer(
        &mut self,
        path: FileWriterPath,
        type_name: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        if self.writers.contains_key(&path) {
            return Ok(());
        }

        let base_schema = get_arrow_schema(type_name).ok_or_else(|| {
            format!("Custom data type \"{type_name}\" is not registered for Arrow encoding")
        })?;

        let schema = schema_with_data_type_column(base_schema.as_ref(), type_name);

        let schema = if self.catalog_identifier_column {
            Self::schema_with_delta_staging_columns(&schema_with_identifier_column(&schema))
        } else {
            schema
        };

        let file = FeatherFile::create(&path.path, &schema, &self.rotation_config)
            .map_err(|e| format!("Failed to create Feather file for custom {type_name}: {e}"))?;
        self.writers.insert(path, file);
        Ok(())
    }

    /// Encodes a single `CustomData` into a `RecordBatch` with `data_type` column (catalog-compatible).
    pub(crate) fn encode_custom_to_batch(
        custom: &CustomData,
    ) -> Result<RecordBatch, Box<dyn std::error::Error>> {
        let type_name = custom.data.type_name();
        let data_type_json = custom
            .data_type
            .to_persistence_json()
            .map_err(|e| format!("Failed to serialize data_type for persistence: {e}"))?;
        let dt_meta = custom.data_type.metadata_string_map();
        let items: [Arc<dyn CustomDataTrait>; 1] = [Arc::clone(&custom.data)];

        let batch = encode_custom_to_arrow(type_name, &items)
            .map_err(|e| format!("Failed to encode custom data: {e}"))?
            .ok_or_else(|| {
                format!("Custom data type \"{type_name}\" is not registered for Arrow")
            })?;

        let batch = augment_batch_with_data_type_column(
            &batch,
            &data_type_json,
            type_name,
            dt_meta.as_ref(),
        )
        .map_err(|e| e.to_string())?;
        Ok(batch)
    }

    fn schema_with_delta_staging_columns(schema: &Schema) -> Schema {
        let mut fields = schema
            .fields()
            .iter()
            .map(|field| {
                Arc::new(Field::new(
                    field.name().clone(),
                    field.data_type().clone(),
                    field.is_nullable(),
                ))
            })
            .collect::<Vec<_>>();

        if schema.index_of(NAUTILUS_ARROW_METADATA_ID_COLUMN).is_err() {
            fields.push(Arc::new(Field::new(
                NAUTILUS_ARROW_METADATA_ID_COLUMN,
                DataType::Utf8,
                false,
            )));
        }

        if schema
            .index_of(NAUTILUS_ARROW_METADATA_JSON_COLUMN)
            .is_err()
        {
            fields.push(Arc::new(Field::new(
                NAUTILUS_ARROW_METADATA_JSON_COLUMN,
                DataType::Utf8,
                false,
            )));
        }

        Schema::new(fields)
    }

    /// Flushes buffered bytes of every open file to disk.
    ///
    /// This is called automatically based on `flush_interval_ms` if configured, but can also
    /// be called manually by the client. Files stay open, so flushing never starts a new file.
    pub fn flush(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        for file in self.writers.values_mut() {
            file.flush()?;
        }

        self.last_flush_ns = self.clock.timestamp_ns();

        if let Some(error) = self.pending_write_error.take() {
            return Err(error.into());
        }

        Ok(())
    }

    /// Seals every open file so its complete stream is visible as a `.feather` file.
    ///
    /// Writes after sealing open new files, so this is the boundary promotion uses.
    pub fn seal(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let paths = self.writers.keys().cloned().collect::<Vec<_>>();
        let mut first_error = None;

        for path in paths {
            if let Err(e) = self.seal_file(&path) {
                log::error!("Failed to seal Feather file {}: {e}", path.path.display());
                first_error.get_or_insert(e);
            }
        }

        match first_error {
            Some(e) => Err(e),
            None => Ok(()),
        }
    }

    /// Seals all open files and reports any write error recorded since the last flush.
    ///
    /// Writes after closing open new files.
    pub fn close(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        self.seal()?;

        if let Some(error) = self.pending_write_error.take() {
            return Err(error.into());
        }

        Ok(())
    }

    /// Returns whether the writer has been closed (all writers cleared).
    #[must_use]
    pub fn is_closed(&self) -> bool {
        self.writers.is_empty()
    }

    /// Returns information about the current files being written.
    ///
    /// Each entry maps a writer key (`type_str` and optional `instrument_id`) to
    /// its current buffer size and file path.
    #[must_use]
    pub fn get_current_file_info(&self) -> HashMap<String, (u64, String)> {
        let mut info = HashMap::new();

        for (path, file) in &self.writers {
            let key = match &path.instrument_id {
                Some(id) => format!("{}:{}", path.type_str, id),
                None => path.type_str.clone(),
            };

            info.insert(key, (file.size(), path.path.display().to_string()));
        }

        info
    }

    /// Returns the next rotation time for a specific writer key, if set.
    #[must_use]
    pub fn get_next_rotation_time(
        &self,
        type_str: &str,
        instrument_id: Option<&str>,
    ) -> Option<UnixNanos> {
        self.next_rotation_times
            .iter()
            .find(|(k, _)| k.type_str == type_str && k.instrument_id.as_deref() == instrument_id)
            .map(|(_, &v)| v)
    }

    /// Determines whether type T can be written before row-level identifier checks.
    fn should_write_type<T: CatalogPathPrefix>(&self) -> bool {
        self.should_write_prefix(T::path_prefix())
    }

    fn should_write_prefix(&self, record_prefix: &str) -> bool {
        self.included_types
            .as_ref()
            .is_none_or(|included| included.contains(record_prefix))
            && self
                .record_filter
                .as_ref()
                .is_none_or(|filter| filter.contains_prefix(record_prefix))
    }

    fn should_write_record(
        &self,
        record_prefix: &str,
        identifier: Option<&str>,
        instrument_type: Option<&str>,
    ) -> bool {
        self.included_types
            .as_ref()
            .is_none_or(|included| included.contains(record_prefix))
            && self
                .record_filter
                .as_ref()
                .is_none_or(|filter| filter.allows(record_prefix, identifier, instrument_type))
    }

    fn reserve_writer_path(
        &mut self,
        type_str: &str,
        instrument_id: Option<String>,
    ) -> FileWriterPath {
        let timestamp = self.clock.timestamp_ns();

        for sequence in 0.. {
            let path =
                self.build_writer_path(type_str, instrument_id.as_deref(), timestamp, sequence);

            // Files left by an earlier writer in the same directory keep their names
            if path.exists() || path.with_extension(FEATHER_PARTIAL_EXTENSION).exists() {
                continue;
            }

            if self.reserved_paths.insert(path.clone()) {
                return FileWriterPath {
                    path,
                    type_str: type_str.to_string(),
                    instrument_id,
                };
            }
        }

        unreachable!("unbounded writer path sequence exhausted")
    }

    fn build_writer_path(
        &self,
        type_str: &str,
        instrument_id: Option<&str>,
        timestamp: UnixNanos,
        sequence: u64,
    ) -> PathBuf {
        let mut path = self.directory.clone();

        if let Some(type_name) = type_str.strip_prefix("data/custom/") {
            path = path.join("data").join("custom").join(type_name);

            // Use a single flat urisafe segment so the writer path, the stream-promotion
            // parser, and the catalog layout (custom_data_path_components) agree.
            let safe_id = instrument_id
                .map(urisafe_instrument_id)
                .filter(|safe| !safe.is_empty());

            if let Some(safe) = &safe_id {
                path = path.join(safe);
            }

            let file_stem = safe_id.as_deref().unwrap_or(type_name);
            path.join(Self::timestamped_feather_file_name(
                file_stem, timestamp, sequence,
            ))
        } else {
            path = path.join(type_str);

            if let Some(instrument_id) = instrument_id {
                path = path.join(urisafe_instrument_id(instrument_id));
            }

            path.join(Self::timestamped_feather_file_name(
                type_str, timestamp, sequence,
            ))
        }
    }

    fn timestamped_feather_file_name(stem: &str, timestamp: UnixNanos, sequence: u64) -> String {
        if sequence == 0 {
            format!("{stem}_{timestamp}.{FEATHER_EXTENSION}")
        } else {
            format!("{stem}_{timestamp}-{sequence}.{FEATHER_EXTENSION}")
        }
    }

    /// Builds `FileWriterPath` for custom data using `DataType` identifier as folder partition (catalog layout).
    fn get_writer_path_custom(
        &mut self,
        type_name: &str,
        identifier: Option<&str>,
    ) -> Result<FileWriterPath, Box<dyn std::error::Error>> {
        let type_str = format!("data/custom/{type_name}");

        if let Some(existing) = self
            .writers
            .keys()
            .find(|path| path.type_str == type_str && path.instrument_id.as_deref() == identifier)
        {
            return Ok(existing.clone());
        }

        if let Some(schema) = get_arrow_schema(type_name) {
            validate_custom_catalog_schema(type_name, &schema)?;
        }

        Ok(self.reserve_writer_path(&type_str, identifier.map(String::from)))
    }

    /// Generates a key for a `FileWriter` based on type T and optional instrument ID.
    /// Reuses an existing writer key (same `type_str` and `instrument_id`) if present, so we
    /// buffer multiple items in the same file until rotation; otherwise creates a new path with current timestamp.
    fn get_writer_path<T>(&mut self, data: &T) -> Result<FileWriterPath, Box<dyn std::error::Error>>
    where
        T: EncodeToRecordBatch + CatalogPathPrefix,
    {
        let type_str = T::path_prefix();
        let metadata = T::metadata(data);

        let instrument_id = if type_str == InstrumentAny::path_prefix()
            || self.per_instrument_types.contains(type_str)
            || (type_str.starts_with("custom_") && metadata.contains_key(KEY_INSTRUMENT_ID))
        {
            Some(metadata.get(KEY_INSTRUMENT_ID).cloned().ok_or_else(|| {
                format!("Data {type_str} expected instrument_id metadata for per instrument writer")
            })?)
        } else {
            None
        };

        // Reuse existing writer for same (type_str, instrument_id) so we buffer in one file until rotation
        if let Some(existing) = self
            .writers
            .keys()
            .find(|k| k.type_str == type_str && k.instrument_id == instrument_id)
        {
            return Ok(existing.clone());
        }

        Ok(self.reserve_writer_path(type_str, instrument_id))
    }

    /// Writes a Data enum value to the appropriate writer.
    ///
    /// This is a convenience method that routes the Data enum to the appropriate
    /// typed write method.
    pub fn write_data(&mut self, data: Data) -> Result<(), Box<dyn std::error::Error>> {
        match data {
            Data::Instrument(instrument) => self.write(*instrument),
            Data::Quote(quote) => self.write(quote),
            Data::Trade(trade) => self.write(trade),
            Data::Bar(bar) => self.write(bar),
            Data::BookDelta(delta) => self.write(delta),
            Data::BookDepth(depth) => self.write(*depth),
            Data::IndexPrice(price) => self.write(price),
            Data::MarkPrice(price) => self.write(price),
            Data::FundingRate(funding) => self.write(funding),
            Data::InstrumentStatus(status) => self.write(status),
            Data::OptionGreeks(greeks) => self.write(greeks),
            Data::InstrumentClose(close) => self.write(close),
            Data::Custom(custom) => self.write_custom_data(&custom),
            Data::BookDeltas(deltas_api) => {
                // Batch write so chunk_metadata can skip a leading BookAction::Clear sentinel
                self.write_batch(deltas_api.deltas.clone())
            }
            #[cfg(feature = "defi")]
            Data::Defi(_) => Err("Unsupported DeFi data variant for feather writes".into()),
        }
    }

    /// Writes mixed data as typed Arrow batches.
    ///
    /// # Panics
    ///
    /// Panics if the built-in data batch dispatch becomes non-exhaustive.
    #[expect(
        clippy::needless_pass_by_value,
        reason = "the public writer API accepts ownership of each submitted data batch"
    )]
    pub fn write_data_batch(&mut self, data: Vec<Data>) -> Result<(), Box<dyn std::error::Error>> {
        for batch in DataBatch::from_data_vec_grouped(&data)? {
            match &batch {
                DataBatch::Custom(data) => {
                    for custom in data.as_ref() {
                        self.write_custom_data(custom)?;
                    }
                }
                batch => write_builtin_data_batch(self, batch)
                    .expect("built-in data batch dispatch is exhaustive")?,
            }
        }

        Ok(())
    }

    /// Writes supported message bus value.
    pub fn write_any_message(
        &mut self,
        message: &dyn Any,
    ) -> Result<bool, Box<dyn std::error::Error>> {
        let Some(command) = Self::write_command(message) else {
            return Ok(false);
        };

        if let Err(e) = command(self) {
            self.record_write_error(e.to_string());
            return Err(e);
        }

        Ok(true)
    }

    pub(crate) fn record_write_error(&mut self, error: String) {
        self.pending_write_error.get_or_insert(error);
    }

    pub(crate) fn write_command(message: &dyn Any) -> Option<FeatherWriteCommand> {
        macro_rules! try_write {
            ($message:expr, $type:ty) => {
                if let Some(value) = $message.downcast_ref::<$type>() {
                    let value = value.clone();
                    return Some(Box::new(move |writer| writer.write(value)));
                }
            };
        }

        try_write!(message, QuoteTick);
        try_write!(message, TradeTick);
        try_write!(message, Bar);
        try_write!(message, OrderBookDelta);
        try_write!(message, OrderBookDepth);
        try_write!(message, IndexPriceUpdate);
        try_write!(message, MarkPriceUpdate);
        try_write!(message, FundingRateUpdate);
        try_write!(message, InstrumentStatus);
        try_write!(message, OptionGreeks);
        try_write!(message, InstrumentClose);
        try_write!(message, InstrumentAny);
        try_write!(message, AccountState);
        try_write!(message, OrderInitialized);
        try_write!(message, OrderDenied);
        try_write!(message, OrderEmulated);
        try_write!(message, OrderSubmitted);
        try_write!(message, OrderAccepted);
        try_write!(message, OrderRejected);
        try_write!(message, OrderPendingCancel);
        try_write!(message, OrderCanceled);
        try_write!(message, OrderCancelRejected);
        try_write!(message, OrderExpired);
        try_write!(message, OrderTriggered);
        try_write!(message, OrderPendingUpdate);
        try_write!(message, OrderReleased);
        try_write!(message, OrderModifyRejected);
        try_write!(message, OrderUpdated);
        try_write!(message, OrderFilled);
        try_write!(message, OrderFillVoided);
        try_write!(message, PositionOpened);
        try_write!(message, PositionChanged);
        try_write!(message, PositionClosed);
        try_write!(message, PositionAdjusted);
        try_write!(message, OrderSnapshot);
        try_write!(message, PositionSnapshot);
        try_write!(message, OrderStatusReport);
        try_write!(message, FillReport);
        try_write!(message, PositionStatusReport);
        try_write!(message, ExecutionMassStatus);

        if let Some(deltas) = message.downcast_ref::<OrderBookDeltas>() {
            let deltas = deltas.deltas.clone();
            return Some(Box::new(move |writer| writer.write_batch(deltas)));
        }

        if let Some(data) = message.downcast_ref::<Data>() {
            let data = data.clone();
            return Some(Box::new(move |writer| writer.write_data(data)));
        }

        if let Some(custom) = message.downcast_ref::<CustomData>() {
            let custom = custom.clone();
            return Some(Box::new(move |writer| {
                writer.write_data(Data::Custom(custom))
            }));
        }

        if let Some(event) = message.downcast_ref::<OrderEventAny>() {
            let event = event.clone();
            return Some(Box::new(move |writer| writer.write_order_event(&event)));
        }

        if let Some(event) = message.downcast_ref::<PositionEvent>() {
            let event = event.clone();
            return Some(Box::new(move |writer| writer.write_position_event(&event)));
        }

        None
    }

    fn write_order_event(
        &mut self,
        event: &OrderEventAny,
    ) -> Result<(), Box<dyn std::error::Error>> {
        match event {
            OrderEventAny::Initialized(event) => self.write(event.clone()),
            OrderEventAny::Denied(event) => self.write(*event),
            OrderEventAny::Emulated(event) => self.write(*event),
            OrderEventAny::Released(event) => self.write(*event),
            OrderEventAny::Submitted(event) => self.write(*event),
            OrderEventAny::Accepted(event) => self.write(*event),
            OrderEventAny::Rejected(event) => self.write(*event),
            OrderEventAny::Canceled(event) => self.write(*event),
            OrderEventAny::Expired(event) => self.write(*event),
            OrderEventAny::Triggered(event) => self.write(*event),
            OrderEventAny::PendingUpdate(event) => self.write(*event),
            OrderEventAny::PendingCancel(event) => self.write(*event),
            OrderEventAny::ModifyRejected(event) => self.write(*event),
            OrderEventAny::CancelRejected(event) => self.write(*event),
            OrderEventAny::Updated(event) => self.write(*event),
            OrderEventAny::Filled(event) => self.write(event.clone()),
            OrderEventAny::FillVoided(event) => self.write(event.clone()),
        }
    }

    fn write_position_event(
        &mut self,
        event: &PositionEvent,
    ) -> Result<(), Box<dyn std::error::Error>> {
        match event {
            PositionEvent::PositionOpened(event) => self.write(event.clone()),
            PositionEvent::PositionChanged(event) => self.write(event.clone()),
            PositionEvent::PositionClosed(event) => self.write(event.clone()),
            PositionEvent::PositionAdjusted(event) => self.write(*event),
        }
    }

    /// Writes a single custom data value (catalog layout: `data/custom/{type_name}[/{identifier}]`).
    fn write_custom_data(&mut self, custom: &CustomData) -> Result<(), Box<dyn std::error::Error>> {
        let batch = Self::encode_custom_to_batch(custom)?;
        self.write_custom_batch(custom, batch)
    }

    pub(crate) fn write_custom_batch(
        &mut self,
        custom: &CustomData,
        mut batch: RecordBatch,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let type_name = custom.data.type_name();
        let identifier = custom.data_type.identifier().map(String::from);

        if !self.should_write_custom(type_name, identifier.as_deref()) {
            return Ok(());
        }

        let path = self.get_writer_path_custom(type_name, identifier.as_deref())?;
        if !self.writers.contains_key(&path) {
            self.create_custom_writer(path.clone(), type_name)?;
        }

        if self.catalog_identifier_column {
            let metadata_row =
                arrow_metadata_row(batch.schema().metadata(), batch.schema().fields())?;
            batch = record_batch_with_identifier_column(batch, custom.data_type.identifier())?;
            batch = record_batch_with_delta_staged_metadata(
                &batch,
                std::slice::from_ref(&metadata_row),
            )?;
        }

        let mut pending = PendingIo::default();

        self.stage_batch_write(path, &batch, &mut pending)?;
        pending.flush_due = self.flush_is_due();

        self.complete_pending_io(&pending)
    }

    pub(crate) fn should_write_custom(&self, type_name: &str, identifier: Option<&str>) -> bool {
        let record_prefix = format!("custom/{type_name}");
        self.included_types.as_ref().is_none_or(|included| {
            included.contains(type_name)
                || included.contains("custom")
                || included.contains(&record_prefix)
        }) && self
            .record_filter
            .as_ref()
            .is_none_or(|filter| filter.allows(&record_prefix, identifier, None))
    }

    /// Writes an instrument to the appropriate writer.
    ///
    /// Instruments are written to feather files and organized by instrument ID.
    /// This method supports writing instruments that implement `EncodeToRecordBatch` and `CatalogPathPrefix`.
    pub fn write_instrument(
        &mut self,
        instrument: InstrumentAny,
    ) -> Result<(), Box<dyn std::error::Error>> {
        self.write(instrument)
    }

    /// Subscribes to all messages on the message bus (pattern "*").
    ///
    /// This will automatically write all supported data types that are published
    /// on the message bus to the feather files.
    ///
    /// The writer must be wrapped in `Rc<RefCell<>>` to be shareable with the message bus handler.
    ///
    /// Note: Writes are synchronous local file appends.
    pub fn subscribe_to_message_bus(
        writer: Rc<RefCell<Self>>,
    ) -> Result<StreamingSinkSubscription, Box<dyn std::error::Error>> {
        let sink: StreamingDataSink = Box::new(writer);
        Ok(StreamingSinkSubscription::subscribe(
            Rc::new(RefCell::new(sink)),
            None,
        ))
    }

    /// Unsubscribes from message bus.
    pub fn unsubscribe_from_message_bus(handler: &StreamingSinkSubscription) {
        handler.unsubscribe();
    }
}

impl Debug for FeatherWriter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct(stringify!(FeatherWriter))
            .finish_non_exhaustive()
    }
}

/// Drop contract: seals open files so their streams stay readable and promotable.
///
/// Errors are logged, never raised. Call [`FeatherWriter::close`] for deterministic sealing.
impl Drop for FeatherWriter {
    fn drop(&mut self) {
        if !self.writers.is_empty() {
            let _ = self.seal();
        }
    }
}

impl StreamingSink for FeatherWriter {
    fn write_data(&mut self, data: Data) -> anyhow::Result<()> {
        Self::write_data(self, data).map_err(feather_error)
    }

    fn write_batch(&mut self, data: Vec<Data>) -> anyhow::Result<()> {
        Self::write_data_batch(self, data).map_err(feather_error)
    }

    fn write_any(&mut self, message: &dyn Any) -> anyhow::Result<bool> {
        self.write_any_message(message).map_err(feather_error)
    }

    fn flush(&mut self) -> anyhow::Result<()> {
        Self::flush(self).map_err(feather_error)
    }

    fn close(&mut self) -> anyhow::Result<()> {
        Self::close(self).map_err(feather_error)
    }
}

impl StreamingSink for Rc<RefCell<FeatherWriter>> {
    fn write_data(&mut self, data: Data) -> anyhow::Result<()> {
        StreamingSink::write_data(&mut *self.borrow_mut(), data)
    }

    fn write_batch(&mut self, data: Vec<Data>) -> anyhow::Result<()> {
        StreamingSink::write_batch(&mut *self.borrow_mut(), data)
    }

    fn write_any(&mut self, message: &dyn Any) -> anyhow::Result<bool> {
        StreamingSink::write_any(&mut *self.borrow_mut(), message)
    }

    fn flush(&mut self) -> anyhow::Result<()> {
        StreamingSink::flush(&mut *self.borrow_mut())
    }

    fn close(&mut self) -> anyhow::Result<()> {
        StreamingSink::close(&mut *self.borrow_mut())
    }
}

/// Returns the replay identity of a flushed Feather file, so a rerun does not write it twice.
pub(crate) fn feather_replay_identity(
    source_uri: &str,
    source_path: &str,
    content_hash: &str,
    identifiers: Option<&[String]>,
) -> String {
    let mut identifiers = identifiers.map(<[String]>::to_vec);
    if let Some(identifiers) = identifiers.as_mut() {
        identifiers.sort();
        identifiers.dedup();
    }

    let identity = serde_json::json!({
        "source_uri": source_uri,
        "source_path": source_path,
        "content_hash": content_hash,
        "identifiers": identifiers,
    });
    format!(
        "nautilus-feather:{}",
        blake3::hash(identity.to_string().as_bytes()).to_hex(),
    )
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, atomic::Ordering};

    use datafusion::arrow::ipc::reader::StreamReader;
    use nautilus_common::{clock::VirtualClock, live::LiveClock};
    use nautilus_model::{
        data::{Data, QuoteTick, TradeTick},
        enums::AggressorSide,
        identifiers::{InstrumentId, TradeId},
        types::{ERROR_PRICE, Price, Quantity},
    };
    use nautilus_serialization::arrow::{
        ArrowSchemaProvider, DecodeDataFromRecordBatch, EncodeToRecordBatch,
    };
    use rstest::rstest;
    use tempfile::TempDir;

    use super::*;

    fn feather_files(directory: &Path, extension: &str) -> Vec<PathBuf> {
        let mut files = Vec::new();
        let mut stack = vec![directory.to_path_buf()];

        while let Some(path) = stack.pop() {
            for entry in fs::read_dir(&path).unwrap().flatten() {
                let path = entry.path();
                if path.is_dir() {
                    stack.push(path);
                } else if path.to_string_lossy().ends_with(&format!(".{extension}")) {
                    files.push(path);
                }
            }
        }

        files.sort();
        files
    }

    fn read_feather_batches(path: &Path) -> Vec<RecordBatch> {
        StreamReader::try_new(File::open(path).unwrap(), None)
            .unwrap()
            .map(Result::unwrap)
            .collect()
    }

    #[rstest]
    fn test_default_per_instrument_types_match_streaming_feather_writer() {
        assert_eq!(
            FeatherWriter::default_per_instrument_types(),
            HashSet::from([
                "bars".to_string(),
                "order_book_deltas".to_string(),
                "order_book_depths".to_string(),
                "option_greeks".to_string(),
                "quotes".to_string(),
                "trades".to_string(),
                "mark_prices".to_string(),
                "index_prices".to_string(),
                "funding_rates".to_string(),
            ]),
        );
    }

    #[rstest]
    fn test_subscription_receives_typed_quotes_and_unsubscribes() {
        use nautilus_common::msgbus::{MStr, publish_quote};

        let temp_dir = TempDir::new().unwrap();
        let writer = Rc::new(RefCell::new(FeatherWriter::new(
            temp_dir.path().to_path_buf(),
            WriterClock::Test(Arc::new(AtomicU64::new(0))),
            RotationConfig::NoRotation,
            None,
            None,
            Some(0),
        )));

        let quote = QuoteTick::new(
            InstrumentId::from("AUD/USD.SIM"),
            Price::from("1.0"),
            Price::from("1.1"),
            Quantity::from("2"),
            Quantity::from("3"),
            4.into(),
            5.into(),
        );
        let handler = FeatherWriter::subscribe_to_message_bus(writer.clone()).unwrap();
        publish_quote(MStr::topic("data.quotes.AUD/USD.SIM").unwrap(), &quote);
        FeatherWriter::unsubscribe_from_message_bus(&handler);
        publish_quote(MStr::topic("data.quotes.AUD/USD.SIM").unwrap(), &quote);
        writer.borrow_mut().close().unwrap();

        let rows = feather_files(temp_dir.path(), FEATHER_EXTENSION)
            .iter()
            .flat_map(|path| read_feather_batches(path))
            .map(|batch| batch.num_rows())
            .sum::<usize>();
        assert_eq!(rows, 1);
    }

    #[rstest]
    #[case(false)]
    #[case(true)]
    fn test_message_write_error_reaches_flush_or_close(#[case] close: bool) {
        let temp_dir = TempDir::new().unwrap();
        let mut writer = FeatherWriter::new(
            temp_dir.path().to_path_buf(),
            WriterClock::Test(Arc::new(AtomicU64::new(0))),
            RotationConfig::NoRotation,
            None,
            None,
            Some(0),
        );

        let quote = QuoteTick::new(
            InstrumentId::from("AUD/USD.SIM"),
            ERROR_PRICE,
            ERROR_PRICE,
            Quantity::from("1"),
            Quantity::from("2"),
            3.into(),
            4.into(),
        );
        let write_error = writer.write_any_message(&quote).unwrap_err().to_string();

        let error = if close {
            StreamingSink::close(&mut writer)
        } else {
            StreamingSink::flush(&mut writer)
        }
        .unwrap_err();

        assert_eq!(error.to_string(), write_error);
    }

    #[rstest]
    fn test_writer_manager_keys() {
        let temp_dir = TempDir::new().unwrap();

        // Create a test time source
        let clock = WriterClock::Test(Arc::new(AtomicU64::new(0)));
        let timestamp = clock.timestamp_ns();

        let quote_type_str = QuoteTick::path_prefix();

        let mut per_instrument = HashSet::new();
        per_instrument.insert(quote_type_str.to_string());

        let mut manager = FeatherWriter::new(
            temp_dir.path().to_path_buf(),
            clock,
            RotationConfig::NoRotation,
            None,
            Some(per_instrument),
            None, // flush_interval_ms
        );

        let instrument_id = "AAPL.AAPL";

        // Write a dummy value
        let quote = QuoteTick::new(
            InstrumentId::from(instrument_id),
            Price::from("100.0"),
            Price::from("100.0"),
            Quantity::from("100.0"),
            Quantity::from("100.0"),
            UnixNanos::from(1_000_000_000_000_000_000),
            UnixNanos::from(1_000_000_000_000_000_000),
        );

        let trade = TradeTick::new(
            InstrumentId::from(instrument_id),
            Price::from("100.0"),
            Quantity::from("100.0"),
            AggressorSide::Buy,
            TradeId::from("1"),
            UnixNanos::from(1_000_000_000_000_000_000),
            UnixNanos::from(1_000_000_000_000_000_000),
        );

        manager.write(quote).unwrap();
        manager.write(trade).unwrap();

        // Check keys and paths for quotes and trades
        let path = manager.get_writer_path(&quote).unwrap();
        let safe_id = instrument_id.replace('/', "");
        let expected_path = temp_dir
            .path()
            .join("quotes")
            .join(&safe_id)
            .join(format!("quotes_{timestamp}.feather"));
        assert_eq!(path.path, expected_path);
        assert!(manager.writers.contains_key(&path));
        let writer = manager.writers.get(&path).unwrap();
        assert!(writer.size() > 0);

        let path = manager.get_writer_path(&trade).unwrap();
        let expected_path = temp_dir
            .path()
            .join("trades")
            .join(format!("trades_{timestamp}.feather"));
        assert_eq!(path.path, expected_path);
        assert!(manager.writers.contains_key(&path));
        let writer = manager.writers.get(&path).unwrap();
        assert!(writer.size() > 0);
    }

    #[rstest]
    fn test_per_instrument_path_keeps_long_id_out_of_filename() {
        let temp_dir = TempDir::new().unwrap();
        let clock = WriterClock::Test(Arc::new(AtomicU64::new(0)));

        let manager = FeatherWriter::new(
            temp_dir.path().to_path_buf(),
            clock,
            RotationConfig::NoRotation,
            None,
            None,
            None,
        );
        let instrument_id = format!("{}.VENUE", "A".repeat(240));

        let path = manager.build_writer_path(
            QuoteTick::path_prefix(),
            Some(&instrument_id),
            UnixNanos::default(),
            0,
        );

        let safe_id = urisafe_instrument_id(&instrument_id);
        let expected = temp_dir
            .path()
            .join("quotes")
            .join(&safe_id)
            .join("quotes_0.feather");
        assert_eq!(path, expected);
    }

    #[rstest]
    fn existing_feather_writer_implements_streaming_data_sink() {
        let temp_dir = TempDir::new().unwrap();
        let clock = WriterClock::Test(Arc::new(AtomicU64::new(0)));

        let mut writer = FeatherWriter::new(
            temp_dir.path().to_path_buf(),
            clock,
            RotationConfig::NoRotation,
            None,
            Some(HashSet::from(["quotes".to_string()])),
            None,
        );

        let quote = QuoteTick::new(
            InstrumentId::from("AUD/USD.SIM"),
            Price::from("1.0"),
            Price::from("1.1"),
            Quantity::from("1000"),
            Quantity::from("1000"),
            UnixNanos::from(1_000),
            UnixNanos::from(1_000),
        );

        StreamingSink::write_data(&mut writer, Data::Quote(quote)).unwrap();
        StreamingSink::flush(&mut writer).unwrap();
        StreamingSink::close(&mut writer).unwrap();

        let files = feather_files(&temp_dir.path().join("quotes"), FEATHER_EXTENSION);
        assert_eq!(files.len(), 1);
        assert!(
            files[0]
                .components()
                .any(|component| component.as_os_str() == "AUDUSD.SIM"),
        );
    }

    #[rstest]
    fn scheduled_rotation_keeps_time_of_day_anchor_after_late_rotation() {
        let temp_dir = TempDir::new().unwrap();

        let now = Arc::new(AtomicU64::new(
            1_767_258_000_000_000_000, // 2026-01-01 09:00:00 UTC
        ));

        let path = FileWriterPath {
            path: PathBuf::from("quotes.feather"),
            type_str: "quotes".to_string(),
            instrument_id: None,
        };

        let mut writer = FeatherWriter::new(
            temp_dir.path().to_path_buf(),
            WriterClock::Test(Arc::clone(&now)),
            RotationConfig::ScheduledDates {
                interval_ns: 86_400_000_000_000,
                rotation_time: UnixNanos::from(36_000_000_000_000u64),
                rotation_timezone: jiff::tz::TimeZone::UTC,
            },
            None,
            None,
            None,
        );

        assert!(!writer.check_scheduled_rotation(&path));
        assert_eq!(
            writer.next_rotation_times[&path],
            UnixNanos::from(1_767_261_600_000_000_000u64),
        );

        now.store(1_767_263_400_000_000_000, Ordering::Relaxed); // 10:30 UTC
        assert!(writer.check_scheduled_rotation(&path));
        assert_eq!(
            writer.next_rotation_times[&path],
            UnixNanos::from(1_767_348_000_000_000_000u64),
        );
    }

    #[rstest]
    fn test_file_writer_round_trip() {
        let instrument_id = "AAPL.AAPL";

        // Write a dummy value.
        let quote = QuoteTick::new(
            InstrumentId::from(instrument_id),
            Price::from("100.0"),
            Price::from("100.0"),
            Quantity::from("100.0"),
            Quantity::from("100.0"),
            UnixNanos::from(100),
            UnixNanos::from(100),
        );
        let metadata = QuoteTick::metadata(&quote);
        let schema = QuoteTick::get_schema(Some(metadata.clone()));
        let batch = QuoteTick::encode_batch(&QuoteTick::metadata(&quote), &[quote]).unwrap();

        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join("quotes.feather");
        let mut file = FeatherFile::create(&path, &schema, &RotationConfig::NoRotation).unwrap();
        file.write_record_batch(&batch).unwrap();
        file.seal().unwrap();

        assert!(!path.with_extension(FEATHER_PARTIAL_EXTENSION).exists());
        let mut reader = StreamReader::try_new(File::open(&path).unwrap(), None).unwrap();

        let read_metadata = reader.schema().metadata().clone();
        assert_eq!(read_metadata, metadata);

        let read_batch = reader.next().unwrap().unwrap();
        assert_eq!(read_batch.column(0), batch.column(0));

        let decoded = QuoteTick::decode_data_batch(&metadata, batch).unwrap();
        assert_eq!(decoded[0], Data::from(quote));
    }

    #[rstest]
    fn test_round_trip() {
        // Create a temporary directory for base path
        let temp_dir = TempDir::new_in(".").unwrap();

        // Create a test time source
        let clock = WriterClock::Test(Arc::new(AtomicU64::new(0)));

        let quote_type_str = QuoteTick::path_prefix();
        let trade_type_str = TradeTick::path_prefix();

        let mut per_instrument = HashSet::new();
        per_instrument.insert(quote_type_str.to_string());
        per_instrument.insert(trade_type_str.to_string());

        let mut manager = FeatherWriter::new(
            temp_dir.path().to_path_buf(),
            clock,
            RotationConfig::NoRotation,
            None,
            Some(per_instrument),
            None, // flush_interval_ms
        );

        let instrument_id = "AAPL.AAPL";

        // Write a dummy value.
        let quote = QuoteTick::new(
            InstrumentId::from(instrument_id),
            Price::from("100.0"),
            Price::from("100.0"),
            Quantity::from("100.0"),
            Quantity::from("100.0"),
            UnixNanos::from(100),
            UnixNanos::from(100),
        );

        let trade = TradeTick::new(
            InstrumentId::from(instrument_id),
            Price::from("100.0"),
            Quantity::from("100.0"),
            AggressorSide::Buy,
            TradeId::from("1"),
            UnixNanos::from(100),
            UnixNanos::from(100),
        );

        manager.write(quote).unwrap();
        manager.write(trade).unwrap();

        let paths = manager.writers.keys().cloned().collect::<Vec<_>>();
        assert_eq!(paths.len(), 2);

        manager.close().unwrap();

        let mut recovered_quotes = Vec::new();
        let mut recovered_trades = Vec::new();

        for path in paths {
            let path_str = path.path;
            let buffer = File::open(&path_str).unwrap();
            let reader = StreamReader::try_new(buffer, None).unwrap();
            let metadata = reader.schema().metadata().clone();
            for batch in reader {
                let batch = batch.unwrap();
                if path_str.to_str().unwrap().contains("quotes") {
                    let decoded = QuoteTick::decode_data_batch(&metadata, batch).unwrap();
                    recovered_quotes.extend(decoded);
                } else if path_str.to_str().unwrap().contains("trades") {
                    let decoded = TradeTick::decode_data_batch(&metadata, batch).unwrap();
                    recovered_trades.extend(decoded);
                }
            }
        }

        // Assert that the recovered data matches the written data
        assert_eq!(recovered_quotes.len(), 1, "Expected one QuoteTick record");
        assert_eq!(recovered_trades.len(), 1, "Expected one TradeTick record");

        // Check key fields to ensure the data round-tripped correctly
        assert_eq!(recovered_quotes[0], Data::from(quote));
        assert_eq!(recovered_trades[0], Data::from(trade));
    }

    #[rstest]
    fn test_write_data_enum() {
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

        // Test writing via write_data
        writer.write_data(Data::Quote(quote)).unwrap();
        writer.flush().unwrap();

        // Verify file was created
        assert!(!writer.writers.is_empty() || temp_dir.path().read_dir().unwrap().count() > 0);
    }

    #[rstest]
    fn test_write_data_all_types() {
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

        // Test all data types
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

        writer.flush().unwrap();
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

        // Write first quote; interval not elapsed, so nothing is flushed yet
        writer.write(quote).unwrap();
        assert_eq!(writer.writers.len(), 1);
        assert_eq!(writer.last_flush_ns, UnixNanos::from(0));

        // Advance the shared time source past the 100ms flush interval
        shared_time.store(200_000_000, Ordering::Relaxed);

        // Second write hits the flush boundary: both quotes reach the same open file
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

        let partial = feather_files(temp_dir.path(), FEATHER_PARTIAL_EXTENSION);
        let flushed_rows = read_feather_batches(&partial[0])
            .iter()
            .map(RecordBatch::num_rows)
            .sum::<usize>();
        assert_eq!(writer.writers.len(), 1);
        assert_eq!(writer.last_flush_ns, UnixNanos::from(200_000_000));
        assert_eq!(partial.len(), 1);
        assert_eq!(flushed_rows, 2);
        assert!(feather_files(temp_dir.path(), FEATHER_EXTENSION).is_empty());
    }

    #[rstest]
    fn flushes_append_to_one_file_per_type_until_close() {
        let temp_dir = TempDir::new().unwrap();
        let shared_time = Arc::new(AtomicU64::new(0));
        let mut writer = FeatherWriter::new(
            temp_dir.path().to_path_buf(),
            WriterClock::Test(Arc::clone(&shared_time)),
            RotationConfig::NoRotation,
            None,
            Some(FeatherWriter::default_per_instrument_types()),
            Some(1_000),
        );
        let quotes = (1..=10)
            .map(|minute| {
                QuoteTick::new(
                    InstrumentId::from("AUD/USD.SIM"),
                    Price::from("1.0"),
                    Price::from("1.1"),
                    Quantity::from("1000"),
                    Quantity::from("1000"),
                    UnixNanos::from(minute * 60_000_000_000),
                    UnixNanos::from(minute * 60_000_000_000),
                )
            })
            .collect::<Vec<_>>();

        // Each write crosses a flush interval, as a 1-minute bar does in a backtest
        for quote in &quotes {
            shared_time.store(quote.ts_init.as_u64(), Ordering::Relaxed);
            writer.write(*quote).unwrap();
        }

        let partial_before_close = feather_files(temp_dir.path(), FEATHER_PARTIAL_EXTENSION);
        let sealed_before_close = feather_files(temp_dir.path(), FEATHER_EXTENSION);
        writer.close().unwrap();
        let sealed = feather_files(temp_dir.path(), FEATHER_EXTENSION);
        let batches = read_feather_batches(&sealed[0]);
        let metadata = batches[0].schema().metadata().clone();
        let recovered = batches
            .into_iter()
            .flat_map(|batch| QuoteTick::decode_data_batch(&metadata, batch).unwrap())
            .collect::<Vec<_>>();

        assert_eq!(partial_before_close.len(), 1);
        assert!(sealed_before_close.is_empty());
        assert_eq!(sealed.len(), 1);
        assert!(feather_files(temp_dir.path(), FEATHER_PARTIAL_EXTENSION).is_empty());
        assert_eq!(
            recovered,
            quotes.into_iter().map(Data::from).collect::<Vec<_>>()
        );
    }

    #[rstest]
    fn drop_seals_open_files() {
        let temp_dir = TempDir::new().unwrap();
        let mut writer = FeatherWriter::new(
            temp_dir.path().to_path_buf(),
            WriterClock::Test(Arc::new(AtomicU64::new(0))),
            RotationConfig::NoRotation,
            None,
            None,
            None,
        );
        writer
            .write(QuoteTick::new(
                InstrumentId::from("AUD/USD.SIM"),
                Price::from("1.0"),
                Price::from("1.1"),
                Quantity::from("1000"),
                Quantity::from("1000"),
                UnixNanos::from(1_000),
                UnixNanos::from(1_000),
            ))
            .unwrap();

        drop(writer);

        assert_eq!(feather_files(temp_dir.path(), FEATHER_EXTENSION).len(), 1);
        assert!(feather_files(temp_dir.path(), FEATHER_PARTIAL_EXTENSION).is_empty());
    }

    #[rstest]
    fn reserved_paths_skip_files_left_in_the_directory() {
        let temp_dir = TempDir::new().unwrap();
        let mut writer = FeatherWriter::new(
            temp_dir.path().to_path_buf(),
            WriterClock::Test(Arc::new(AtomicU64::new(5))),
            RotationConfig::NoRotation,
            None,
            None,
            None,
        );
        let sealed = temp_dir.path().join("trades").join("trades_5.feather");
        let partial = temp_dir
            .path()
            .join("trades")
            .join("trades_5-1.feather.partial");
        fs::create_dir_all(sealed.parent().unwrap()).unwrap();
        fs::write(&sealed, b"sealed").unwrap();
        fs::write(&partial, b"partial").unwrap();

        let path = writer.reserve_writer_path("trades", None);

        assert_eq!(
            path.path,
            temp_dir.path().join("trades").join("trades_5-2.feather")
        );
        assert_eq!(fs::read(&sealed).unwrap(), b"sealed");
        assert_eq!(fs::read(&partial).unwrap(), b"partial");
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
        assert!(!writer.writers.is_empty());

        writer.close().unwrap();
        assert!(writer.writers.is_empty());
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
        writer.flush().unwrap();
    }

    #[rstest]
    fn feather_writer_is_send() {
        fn assert_send<T: Send>() {}
        assert_send::<FeatherWriter>();
        assert_send::<WriterClock>();
    }

    #[rstest]
    fn writer_clock_test_source_reads_shared_atomic() {
        let shared = Arc::new(AtomicU64::new(7));
        let clock = WriterClock::Test(Arc::clone(&shared));
        assert_eq!(clock.timestamp_ns(), UnixNanos::from(7));

        shared.store(42, Ordering::Relaxed);
        assert_eq!(clock.timestamp_ns(), UnixNanos::from(42));
    }

    #[rstest]
    fn writer_clock_from_shared_clock_wires_test_clock() {
        let test_clock = Rc::new(RefCell::new(VirtualClock::new()));
        test_clock
            .borrow_mut()
            .advance_time(UnixNanos::from(42), true);
        let clock: Rc<RefCell<dyn Clock>> = test_clock.clone();

        let (writer_clock, shared) = WriterClock::from_shared_clock(&clock);
        let shared = shared.expect("non-live clocks must return a shared atomic");

        // Seeded with the source clock's current time
        assert_eq!(writer_clock.timestamp_ns(), UnixNanos::from(42));

        // Bridge refresh: advancing the source clock and storing into the atomic
        // is what the PyO3 wrapper does before each forwarded call
        test_clock
            .borrow_mut()
            .advance_time(UnixNanos::from(99), true);
        shared.store(clock.borrow().timestamp_ns().as_u64(), Ordering::Relaxed);
        assert_eq!(writer_clock.timestamp_ns(), UnixNanos::from(99));
    }

    #[rstest]
    fn writer_clock_from_shared_clock_live_clock_is_live() {
        let clock: Rc<RefCell<dyn Clock>> = Rc::new(RefCell::new(LiveClock::new(None)));

        let (writer_clock, shared) = WriterClock::from_shared_clock(&clock);

        assert!(matches!(writer_clock, WriterClock::Live));
        assert!(shared.is_none());
    }

    #[rstest]
    fn size_rotation_seals_each_full_file_and_opens_the_next_lazily() {
        let temp_dir = TempDir::new().unwrap();
        let shared_clock = Arc::new(AtomicU64::new(0));
        let mut writer = FeatherWriter::new(
            temp_dir.path().to_path_buf(),
            WriterClock::Test(Arc::clone(&shared_clock)),
            RotationConfig::Size { max_size: 1 },
            None,
            None,
            Some(1),
        );

        for ts in [1_000, 2_000] {
            shared_clock.store(ts * 1_000_000, Ordering::Relaxed);
            writer
                .write(QuoteTick::new(
                    InstrumentId::from("AUD/USD.SIM"),
                    Price::from("1.0"),
                    Price::from("1.0"),
                    Quantity::from("1000"),
                    Quantity::from("1000"),
                    UnixNanos::from(ts),
                    UnixNanos::from(ts),
                ))
                .unwrap();
        }

        let rows = feather_files(temp_dir.path(), FEATHER_EXTENSION)
            .iter()
            .map(|path| {
                read_feather_batches(path)
                    .iter()
                    .map(RecordBatch::num_rows)
                    .sum::<usize>()
            })
            .collect::<Vec<_>>();
        assert!(writer.writers.is_empty());
        assert_eq!(rows, vec![1, 1]);
        assert!(feather_files(temp_dir.path(), FEATHER_PARTIAL_EXTENSION).is_empty());
    }

    #[rstest]
    #[case(
        "FeatherMissingTimestamp",
        Schema::empty(),
        "registered without an Arrow schema containing ts_init"
    )]
    #[case(
        "FeatherLegacyTimestamp",
        Schema::new(vec![Field::new("ts_init", DataType::UInt64, false)]),
        "registered with ts_init as UInt64",
    )]
    fn test_custom_writer_path_rejects_unqueryable_schema(
        #[case] type_name: &str,
        #[case] schema: Schema,
        #[case] expected_error: &str,
    ) {
        nautilus_model::data::registry::ensure_arrow_registered(
            type_name,
            Arc::new(schema),
            Box::new(|_| unreachable!("writer creation does not encode data")),
            Box::new(|_, _| unreachable!("writer creation does not decode data")),
        )
        .unwrap();

        let temp_dir = TempDir::new().unwrap();
        let mut writer = FeatherWriter::new(
            temp_dir.path().to_path_buf(),
            WriterClock::Test(Arc::new(AtomicU64::new(0))),
            RotationConfig::NoRotation,
            None,
            None,
            None,
        );

        for _ in 0..2 {
            let error = writer.get_writer_path_custom(type_name, None).unwrap_err();

            assert!(error.to_string().contains(expected_error));
            assert!(writer.writers.is_empty());
            assert!(writer.reserved_paths.is_empty());
        }
    }

    #[rstest]
    #[cfg(feature = "python")]
    fn test_write_custom_data_round_trip() {
        use std::sync::Arc;

        use nautilus_model::{
            data::{CustomData, Data, DataType},
            identifiers::InstrumentId,
        };
        use nautilus_serialization::{
            arrow::custom::CustomDataDecoder, ensure_custom_data_registered,
        };

        use crate::test_data::RustTestCustomData;

        ensure_custom_data_registered::<RustTestCustomData>();

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

        let instrument_id = InstrumentId::from("RUST.TEST");
        let data_type = DataType::new("RustTestCustomData", None, Some(instrument_id.to_string()));

        let original = RustTestCustomData {
            instrument_id,
            value: 1.23,
            flag: true,
            ts_event: UnixNanos::from(1000),
            ts_init: UnixNanos::from(1000),
        };

        let custom = CustomData::new(Arc::new(original.clone()), data_type);

        writer
            .write_data(Data::Custom(custom))
            .expect("write_data CustomData");
        writer.close().expect("close");

        let prefix = temp_dir
            .path()
            .join("data")
            .join("custom")
            .join("RustTestCustomData");
        let files = feather_files(&prefix, FEATHER_EXTENSION);
        let mut reader =
            StreamReader::try_new(File::open(&files[0]).unwrap(), None).expect("StreamReader");
        let schema = reader.schema();
        let metadata: std::collections::HashMap<String, String> = schema
            .metadata()
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        let batch = reader.next().expect("batch").expect("batch ok");
        let decoded =
            CustomDataDecoder::decode_data_batch(&metadata, batch).expect("decode_data_batch");
        assert_eq!(decoded.len(), 1);

        if let Data::Custom(decoded_custom) = &decoded[0] {
            assert_eq!(decoded_custom.data_type.type_name(), "RustTestCustomData");
            let rust: &RustTestCustomData = decoded_custom
                .data
                .as_any()
                .downcast_ref::<RustTestCustomData>()
                .expect("RustTestCustomData");
            assert_eq!(rust, &original);
        } else {
            panic!("Expected Data::Custom");
        }
    }

    #[rstest]
    #[cfg(feature = "python")]
    fn test_write_custom_data_reuses_writer_until_rotation() {
        use nautilus_model::data::{CustomData, DataType};
        use nautilus_serialization::ensure_custom_data_registered;

        use crate::test_data::RustTestCustomData;

        ensure_custom_data_registered::<RustTestCustomData>();
        let temp_dir = TempDir::new().unwrap();

        let mut writer = FeatherWriter::new(
            temp_dir.path().to_path_buf(),
            WriterClock::Test(Arc::new(AtomicU64::new(0))),
            RotationConfig::NoRotation,
            None,
            None,
            None,
        );
        let instrument_id = InstrumentId::from("RUST.TEST");
        let data_type = DataType::new("RustTestCustomData", None, Some(instrument_id.to_string()));

        for (ts, value) in [(1_000, 1_000.0), (2_000, 2_000.0)] {
            writer
                .write_data(Data::Custom(CustomData::new(
                    Arc::new(RustTestCustomData {
                        instrument_id,
                        value,
                        flag: true,
                        ts_event: UnixNanos::from(ts),
                        ts_init: UnixNanos::from(ts),
                    }),
                    data_type.clone(),
                )))
                .unwrap();
        }

        assert_eq!(writer.get_current_file_info().len(), 1);
        writer.close().unwrap();

        let prefix = temp_dir
            .path()
            .join("data")
            .join("custom")
            .join("RustTestCustomData");
        let files = feather_files(&prefix, FEATHER_EXTENSION);
        assert_eq!(files.len(), 1);
        let rows = read_feather_batches(&files[0])
            .iter()
            .map(RecordBatch::num_rows)
            .sum::<usize>();
        assert_eq!(rows, 2);
    }

    #[rstest]
    fn feather_replay_identity_is_stable_and_ignores_identifier_order() {
        let identity = |identifiers: Option<&[String]>| {
            feather_replay_identity(
                "file:///catalog/backtest/run-1",
                "quotes/AUDUSD.SIM/part-0.feather",
                "content-hash",
                identifiers,
            )
        };

        let unordered = ["B".to_string(), "A".to_string(), "A".to_string()];
        let ordered = ["A".to_string(), "B".to_string()];

        let expected =
            "nautilus-feather:10a9435c28f7536f26653c3fc808571be89bfe769e06c7307cb4743e273a23fd";

        assert_eq!(identity(Some(&unordered)), expected);
        assert_eq!(identity(Some(&ordered)), expected);
        assert_ne!(identity(None), expected);
    }
}
