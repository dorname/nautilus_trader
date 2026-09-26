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

//! Feather-file session reading and stream-to-parquet conversion.
//!
//! Methods for reading per-run feather files written by the live/backtest writers and
//! converting them to consolidated parquet for catalog ingest.

#![expect(
    clippy::unused_self,
    reason = "session registration keeps backend-specific ordering logic together"
)]

use std::{borrow::Cow, collections::HashMap, sync::Arc};

use datafusion::arrow::{
    array::{Array, FixedSizeListArray, LargeListArray, ListArray, StructArray, UInt64Array},
    compute::{SortColumn, SortOptions, concat_batches, lexsort_to_indices, take_record_batch},
    datatypes::{DataType, Schema},
    record_batch::RecordBatch,
};
use futures::StreamExt;
use indexmap::IndexMap;
use nautilus_common::enums::Environment;
use nautilus_core::UnixNanos;
use nautilus_model::data::{
    Bar, Data, FundingRateUpdate, HasTsInit, IndexPriceUpdate, InstrumentStatus, MarkPriceUpdate,
    OptionGreeks, OrderBookDelta, OrderBookDepth, QuoteTick, TradeTick, close::InstrumentClose,
    to_variant,
};
use nautilus_serialization::arrow::{
    DecodeDataFromRecordBatch, DecodeTypedFromRecordBatch, U64ColumnRef,
};
use object_store::path::Path as ObjectPath;

use crate::{
    backend::parquet::{
        catalog::ParquetDataCatalog,
        intervals::are_intervals_disjoint,
        paths::{catalog_filename, make_object_store_path, urisafe_instrument_id},
    },
    catalog::types::{
        CatalogDataType, instrument_path_prefix, parquet_data_path_prefix, record_path_prefix,
    },
    common::{
        conversion::FeatherConversionSummary,
        custom::decode_custom_batches_to_data,
        datafusion::{filter_record_batch_by_identifier, identifiers_from_record_batches},
        paths::{
            environment_directory, identifier_from_session_feather_path, local_writer_directory,
            type_name_from_session_feather_path,
        },
    },
    writer::{
        materializer::{
            StreamConversionOptions, apply_stream_conversion_transform,
            coalesce_stream_conversion_batches, read_feather_record_batches,
            restore_staged_record_batches,
        },
        run::FeatherSessionSource,
    },
};

impl ParquetDataCatalog {
    pub(crate) fn promote_feather_file(
        &self,
        source: &FeatherSessionSource,
        feather_path: &str,
        batches: Vec<RecordBatch>,
        use_ts_event_for_ts_init: bool,
        replay_identity: &str,
    ) -> anyhow::Result<Option<FeatherConversionSummary>> {
        let batches = Self::restore_staged_batches(batches)?;
        if batches.is_empty() {
            return Ok(None);
        }

        let type_name =
            type_name_from_session_feather_path(feather_path, &source.kind, &source.instance_id)?;
        let catalog_data_name = Self::canonical_stream_data_name(&type_name);
        anyhow::ensure!(
            Self::is_supported_stream_data_type(catalog_data_name),
            "Unknown data class: {type_name}"
        );

        let identifier = Self::identifier_from_batch_or_path(
            &batches[0],
            feather_path,
            &source.kind,
            &source.instance_id,
        )
        .filter(|identifier| {
            batches.iter().all(|batch| {
                Self::identifier_from_batch_or_path(
                    batch,
                    feather_path,
                    &source.kind,
                    &source.instance_id,
                )
                .as_ref()
                    == Some(identifier)
            })
        });

        self.convert_feather_batches_to_parquet(
            &source.kind,
            &source.instance_id,
            catalog_data_name,
            feather_path,
            &batches,
            use_ts_event_for_ts_init,
            Some(replay_identity),
        )?;
        Ok(Some(FeatherConversionSummary {
            type_name,
            identifier,
            feather_path: feather_path.to_string(),
            native_version: None,
            unmatched_identifiers: None,
        }))
    }

    /// Reads data from a live run instance.
    ///
    /// This method reads all data associated with a specific live run instance
    /// from feather files stored in the catalog.
    ///
    /// # Parameters
    ///
    /// - `instance_id`: The ID of the live run instance to read.
    ///
    /// # Returns
    ///
    /// Returns a vector of `Data` objects from the live run, sorted by timestamp,
    /// or an error if the operation fails.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The instance ID doesn't exist.
    /// - Feather file reading fails.
    /// - Data deserialization fails.
    ///
    /// # Note
    ///
    /// This method reads through the run reader: it lists the run's data-type directories, reads
    /// every Feather file through the Arrow IPC stream reader with staged batch restoration, decodes
    /// quotes, trades, order book deltas and depths, bars, index and mark prices, option Greeks,
    /// funding rates, instrument status and closes, and custom data files into `Data` values, skips
    /// unknown data types, and sorts the result by `ts_init`.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use nautilus_persistence::backend::parquet::catalog::ParquetDataCatalog;
    ///
    /// let mut catalog = ParquetDataCatalog::new(
    ///     std::path::Path::new("/tmp/nautilus_data"),
    ///     None,
    ///     None,
    ///     None,
    ///     None,
    /// );
    ///
    /// // Read data from a live run
    /// let data = catalog.read_live_run("instance-123")?;
    /// for item in data {
    ///     println!("Data: {:?}", item);
    /// }
    /// # Ok::<(), anyhow::Error>(())
    /// ```
    pub fn read_live_run(&self, instance_id: &str) -> anyhow::Result<Vec<Data>> {
        self.read_run("live", instance_id, None, None, None, None)
    }

    /// Reads data from a backtest run instance.
    ///
    /// This method reads all data associated with a specific backtest run instance
    /// from feather files stored in the catalog.
    ///
    /// # Parameters
    ///
    /// - `instance_id`: The ID of the backtest run instance to read.
    ///
    /// # Returns
    ///
    /// Returns a vector of `Data` objects from the backtest run, sorted by timestamp,
    /// or an error if the operation fails.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The instance ID doesn't exist.
    /// - Feather file reading fails.
    /// - Data deserialization fails.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use nautilus_persistence::backend::parquet::catalog::ParquetDataCatalog;
    ///
    /// let mut catalog = ParquetDataCatalog::new(
    ///     std::path::Path::new("/tmp/nautilus_data"),
    ///     None,
    ///     None,
    ///     None,
    ///     None,
    /// );
    ///
    /// // Read data from a backtest run
    /// let data = catalog.read_backtest("instance-123")?;
    /// for item in data {
    ///     println!("Data: {:?}", item);
    /// }
    /// # Ok::<(), anyhow::Error>(())
    /// ```
    pub fn read_backtest(&self, instance_id: &str) -> anyhow::Result<Vec<Data>> {
        self.read_run("backtest", instance_id, None, None, None, None)
    }

    /// Reads the sealed Feather files of a run instance (backtest or live).
    ///
    /// `data_types` limits the read to those stream families, `identifiers` to the records whose
    /// identifier (instrument ID, bar type, or custom data identifier) contains one of them, and
    /// `start`/`end` to the inclusive `ts_init` range. Every data type is read for every matching
    /// identifier. Records without an identifier pass the identifier filter. Without
    /// `data_types`, families that do not decode to `Data`, such as order events, are skipped.
    ///
    /// # Errors
    ///
    /// Returns an error if a requested data type is not a readable stream family, or if listing,
    /// reading, or decoding a Feather file fails.
    pub(crate) fn read_run(
        &self,
        subdirectory: &str,
        instance_id: &str,
        data_types: Option<&[CatalogDataType]>,
        identifiers: Option<&[String]>,
        start: Option<UnixNanos>,
        end: Option<UnixNanos>,
    ) -> anyhow::Result<Vec<Data>> {
        let data_names = data_types
            .map(|data_types| {
                data_types
                    .iter()
                    .map(|data_type| {
                        let data_name = Self::stream_data_name(data_type)?;
                        // Decoding no batches checks the family without reading a file
                        if self.decode_run_batches(&data_name, Vec::new())?.is_none() {
                            anyhow::bail!("Cannot read {data_type} from a Feather run as data");
                        }
                        Ok(data_name.into_owned())
                    })
                    .collect::<anyhow::Result<Vec<_>>>()
            })
            .transpose()?;
        let feather_files =
            self.list_feather_files(subdirectory, instance_id, None, identifiers)?;
        let mut all_data: Vec<Data> = Vec::new();

        for file_path in feather_files {
            let data_name =
                type_name_from_session_feather_path(&file_path, subdirectory, instance_id)?;

            if data_names
                .as_ref()
                .is_some_and(|names| !names.contains(&data_name))
            {
                continue;
            }

            // A file may hold several batches
            let mut batches = self.read_feather_file(&file_path)?;

            // Catalog writers stage every identifier of a type in one file, so the listing
            // cannot filter them by folder and rows are filtered by their identifier column
            if let Some(identifiers) = identifiers {
                let path_identifier =
                    identifier_from_session_feather_path(&file_path, subdirectory, instance_id);
                batches = batches
                    .iter()
                    .filter_map(|batch| {
                        filter_record_batch_by_identifier(
                            batch,
                            path_identifier.as_deref(),
                            |identifier| {
                                identifier.is_none_or(|identifier| {
                                    Self::stream_identifier_matches(identifier, identifiers)
                                })
                            },
                        )
                        .transpose()
                    })
                    .collect::<anyhow::Result<Vec<_>>>()?;
            }

            if batches.is_empty() {
                continue;
            }

            if let Some(file_data) = self.decode_run_batches(&data_name, batches)? {
                all_data.extend(file_data);
            }
        }

        all_data.retain(|data| {
            let ts_init = data.ts_init();
            start.is_none_or(|start| ts_init >= start) && end.is_none_or(|end| ts_init <= end)
        });
        all_data.sort_by_key(HasTsInit::ts_init);

        Ok(all_data)
    }

    // Returns `None` for stream families that do not decode to `Data`
    fn decode_run_batches(
        &self,
        data_name: &str,
        batches: Vec<RecordBatch>,
    ) -> anyhow::Result<Option<Vec<Data>>> {
        let data = match Self::canonical_stream_data_name(data_name) {
            "quotes" => self
                .convert_record_batches_to_data::<QuoteTick>(batches, false)?
                .into_iter()
                .map(Data::from)
                .collect(),
            "trades" => self
                .convert_record_batches_to_data::<TradeTick>(batches, false)?
                .into_iter()
                .map(Data::from)
                .collect(),
            "order_book_deltas" => self
                .convert_record_batches_to_data::<OrderBookDelta>(batches, false)?
                .into_iter()
                .map(Data::from)
                .collect(),
            "order_book_depths" => self
                .convert_record_batches_to_data::<OrderBookDepth>(batches, false)?
                .into_iter()
                .map(Data::from)
                .collect(),
            "bars" => self
                .convert_record_batches_to_data::<Bar>(batches, false)?
                .into_iter()
                .map(Data::from)
                .collect(),
            "index_prices" => self
                .convert_record_batches_to_data::<IndexPriceUpdate>(batches, false)?
                .into_iter()
                .map(Data::from)
                .collect(),
            "mark_prices" => self
                .convert_record_batches_to_data::<MarkPriceUpdate>(batches, false)?
                .into_iter()
                .map(Data::from)
                .collect(),
            "option_greeks" => self
                .convert_record_batches_to_data::<OptionGreeks>(batches, false)?
                .into_iter()
                .map(Data::from)
                .collect(),
            "funding_rates" => self
                .convert_record_batches_to_data::<FundingRateUpdate>(batches, false)?
                .into_iter()
                .map(Data::from)
                .collect(),
            "instrument_status" => self
                .convert_record_batches_to_data::<InstrumentStatus>(batches, false)?
                .into_iter()
                .map(Data::from)
                .collect(),
            "instrument_closes" => self
                .convert_record_batches_to_data::<InstrumentClose>(batches, false)?
                .into_iter()
                .map(Data::from)
                .collect(),
            name if name.starts_with("custom/") => decode_custom_batches_to_data(batches, false)?,
            _ => return Ok(None),
        };

        Ok(Some(data))
    }

    /// Lists feather files in a subdirectory, for one data class or every class when `data_name`
    /// is `None`.
    ///
    /// This function finds all `.feather` files in the specified subdirectory
    /// (backtest or live) for the given instance ID.
    fn list_feather_files(
        &self,
        subdirectory: &str,
        instance_id: &str,
        data_name: Option<&str>,
        identifiers: Option<&[String]>,
    ) -> anyhow::Result<Vec<String>> {
        let base_dir = make_object_store_path(&self.base_path, [subdirectory, instance_id]);

        let mut files = self.execute_async(|| async {
            let prefix = ObjectPath::from(format!("{base_dir}/"));
            let mut stream = self.object_store.list(Some(&prefix));
            let mut feather_files = Vec::new();

            while let Some(object) = stream.next().await {
                let object = object?;
                let path_str = object.location.to_string();

                if !path_str.ends_with(".feather") {
                    continue;
                }

                let Ok(path_data_name) =
                    type_name_from_session_feather_path(&path_str, subdirectory, instance_id)
                else {
                    continue;
                };

                if data_name.is_some_and(|data_name| path_data_name != data_name) {
                    continue;
                }

                let path_identifier =
                    identifier_from_session_feather_path(&path_str, subdirectory, instance_id);

                if let (Some(identifiers), Some(path_identifier)) =
                    (identifiers, path_identifier.as_deref())
                    && !Self::stream_identifier_matches(path_identifier, identifiers)
                {
                    continue;
                }

                feather_files.push(path_str);
            }

            Ok::<Vec<String>, anyhow::Error>(feather_files)
        })?;

        files.sort();
        Ok(files)
    }

    fn stream_identifier_matches(candidate: &str, identifiers: &[String]) -> bool {
        identifiers.iter().any(|id| {
            let safe_id = urisafe_instrument_id(id);
            candidate.contains(id) || candidate.contains(&safe_id)
        })
    }

    /// Reads a feather file and returns all `RecordBatches`.
    fn read_feather_file(&self, file_path: &str) -> anyhow::Result<Vec<RecordBatch>> {
        let path = ObjectPath::from(file_path);

        let batches = self.execute_async(|| async {
            read_feather_record_batches(self.object_store.clone(), &path).await
        })?;

        Self::restore_staged_batches(batches)
    }

    fn restore_staged_batches(batches: Vec<RecordBatch>) -> anyhow::Result<Vec<RecordBatch>> {
        let mut restored = Vec::new();
        for batch in batches {
            restored.extend(restore_staged_record_batches(batch)?);
        }

        Ok(restored)
    }

    /// Converts `RecordBatches` to Data objects, optionally replacing `ts_init` with `ts_event`.
    fn convert_record_batches_to_data<T>(
        &self,
        batches: Vec<RecordBatch>,
        use_ts_event_for_ts_init: bool,
    ) -> anyhow::Result<Vec<T>>
    where
        T: DecodeDataFromRecordBatch + TryFrom<Data>,
    {
        let mut all_data = Vec::new();

        for batch in batches {
            let batch = apply_stream_conversion_transform(
                &batch,
                StreamConversionOptions {
                    use_ts_event_for_ts_init,
                    convert_bar_type_to_external: false,
                },
            )?;

            let metadata = batch.schema().metadata().clone();

            let data_vec = T::decode_data_batch(&metadata, batch)
                .map_err(|e| anyhow::anyhow!("Failed to decode batch: {e}"))?;

            all_data.extend(data_vec);
        }

        Ok(to_variant::<T>(all_data))
    }

    /// Converts `RecordBatches` directly to strongly typed values.
    pub(crate) fn convert_record_batches_to_typed<T>(
        &self,
        batches: Vec<RecordBatch>,
    ) -> anyhow::Result<Vec<T>>
    where
        T: DecodeTypedFromRecordBatch,
    {
        let mut all_data = Vec::new();

        for batch in batches {
            let metadata = batch.schema().metadata().clone();
            let decoded = T::decode_typed_batch(&metadata, batch)
                .map_err(|e| anyhow::anyhow!("Failed to decode batch: {e}"))?;
            all_data.extend(decoded);
        }

        Ok(all_data)
    }

    /// Converts stream data from feather files to parquet files.
    ///
    /// This method reads data from feather files generated during a backtest or live run
    /// and writes it to the catalog in parquet format. It's useful for converting temporary
    /// stream data into a more permanent and queryable format.
    ///
    /// # Parameters
    ///
    /// - `instance_id`: The ID of the backtest or live run instance.
    /// - `data_cls`: The data class name (e.g., "quotes", "trades", "bars"), or
    ///   `custom/{TypeName}` with the registered type name verbatim for custom data.
    /// - `environment`: The environment the run executed in, which names its folder.
    /// - `identifiers`: Optional list of identifiers to filter by (instrument IDs or bar types).
    /// - `use_ts_event_for_ts_init`: If true, replaces the `ts_init` column with `ts_event` column values before deserializing.
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` on success, or an error if the operation fails.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - `data_type` is an instrument class selector, which has no staged stream name.
    /// - `data_type` is a family streams do not support.
    /// - Feather file listing fails.
    /// - Feather file reading fails.
    /// - Writing to parquet fails.
    ///
    /// # Note
    ///
    /// This method converts directly between Arrow IPC stream batches and Parquet batches without
    /// materializing Nautilus data objects. An instance with no staged files for the family
    /// converts nothing and returns success. It requires:
    /// - Listing feather files in the run folder
    /// - Reading feather files (Arrow IPC stream reading)
    /// - Applying table-only stream conversion transforms
    /// - Writing Arrow batches to the catalog
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use nautilus_common::enums::Environment;
    /// use nautilus_model::data::NautilusDataType;
    /// use nautilus_persistence::backend::parquet::catalog::ParquetDataCatalog;
    ///
    /// let mut catalog = ParquetDataCatalog::new(
    ///     std::path::Path::new("/tmp/nautilus_data"),
    ///     None,
    ///     None,
    ///     None,
    ///     None,
    /// );
    ///
    /// // Convert backtest stream data to parquet
    /// catalog.convert_stream_to_data(
    ///     "instance-123",
    ///     &NautilusDataType::QuoteTick.into(),
    ///     Environment::Backtest,
    ///     None,
    ///     false,
    /// )?;
    /// # Ok::<(), anyhow::Error>(())
    /// ```
    pub fn convert_stream_to_data(
        &mut self,
        instance_id: &str,
        data_type: &CatalogDataType,
        environment: Environment,
        identifiers: Option<&[String]>,
        use_ts_event_for_ts_init: bool,
    ) -> anyhow::Result<()> {
        let subdirectory = environment_directory(environment);
        let stream_data_name = Self::stream_data_name(data_type)?;

        // List all feather files for this data class
        let feather_files = self.list_feather_files(
            subdirectory,
            instance_id,
            Some(&stream_data_name),
            identifiers,
        )?;

        // Process each feather file independently so that each file's identifier
        // (instrument_id or bar_type from schema metadata) is preserved when writing
        // to parquet. Each file is planned before it is written.
        for file_path in feather_files {
            let batches = self.read_feather_file(&file_path)?;
            self.convert_feather_batches_to_parquet(
                subdirectory,
                instance_id,
                &stream_data_name,
                &file_path,
                &batches,
                use_ts_event_for_ts_init,
                None,
            )?;
        }

        Ok(())
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "the arguments describe one Feather source and its catalog destination"
    )]
    fn convert_feather_batches_to_parquet(
        &self,
        subdirectory: &str,
        instance_id: &str,
        catalog_data_name: &str,
        feather_path: &str,
        batches: &[RecordBatch],
        use_ts_event_for_ts_init: bool,
        replay_identity: Option<&str>,
    ) -> anyhow::Result<()> {
        let mut groups: IndexMap<Arc<Schema>, Vec<RecordBatch>> = IndexMap::new();

        for batch in batches {
            groups
                .entry(batch.schema())
                .or_default()
                .push(batch.clone());
        }

        let mut planned = Vec::new();

        for (index, group) in groups.into_values().enumerate() {
            if let Some(plan) = self.plan_catalog_write(
                subdirectory,
                instance_id,
                catalog_data_name,
                feather_path,
                &group,
                use_ts_event_for_ts_init,
                replay_identity,
                index,
            )? {
                planned.push(plan);
            }
        }

        let mut by_directory: IndexMap<String, Vec<PlannedCatalogWrite>> = IndexMap::new();

        for plan in planned {
            by_directory
                .entry(plan.directory.clone())
                .or_default()
                .push(plan);
        }

        let mut ready = Vec::new();

        for (directory, plans) in by_directory {
            ready.extend(self.ready_directory_plans(&directory, plans)?);
        }

        for plan in ready {
            self.write_parquet_file_checked(
                &plan.directory,
                UnixNanos::from(plan.start_ts),
                UnixNanos::from(plan.end_ts),
                std::slice::from_ref(&plan.batch),
                false,
                "File",
                None,
                Some(&plan.group_identity),
            )?;
        }

        Ok(())
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "the arguments describe one restored schema group and its catalog destination"
    )]
    fn plan_catalog_write(
        &self,
        subdirectory: &str,
        instance_id: &str,
        catalog_data_name: &str,
        feather_path: &str,
        group: &[RecordBatch],
        use_ts_event_for_ts_init: bool,
        replay_identity: Option<&str>,
        index: usize,
    ) -> anyhow::Result<Option<PlannedCatalogWrite>> {
        let Some(batch) = Self::apply_stream_conversion_transforms(group, use_ts_event_for_ts_init)
            .map_err(|e| {
                anyhow::anyhow!(
                    "Failed to apply stream conversion transforms for {feather_path}: {e}"
                )
            })?
        else {
            return Ok(None);
        };

        let (start_ts, end_ts) = Self::ts_init_range(&batch).map_err(|e| {
            anyhow::anyhow!("Failed to determine ts_init range for {feather_path}: {e}")
        })?;

        let identifier =
            Self::identifier_from_batch_or_path(&batch, feather_path, subdirectory, instance_id);

        let instrument_prefix = if catalog_data_name == "instruments" {
            let class = batch
                .schema()
                .metadata()
                .get("class")
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("Staged instrument has no class metadata"))?;
            Some(instrument_path_prefix(&class.parse()?))
        } else {
            None
        };

        let catalog_data_name = instrument_prefix.unwrap_or(catalog_data_name);

        let directory = if let Some(type_name) = catalog_data_name.strip_prefix("custom/") {
            self.make_path_custom_data(type_name, identifier.as_deref())?
        } else {
            self.make_path(catalog_data_name, identifier.as_deref())?
        };

        let batch = Self::with_catalog_identifier_metadata(
            batch,
            catalog_data_name,
            identifier.as_deref(),
        )?;

        Ok(Some(PlannedCatalogWrite {
            directory,
            start_ts,
            end_ts,
            batch,
            group_identity: format!("{}/{index}", replay_identity.unwrap_or(feather_path)),
        }))
    }

    fn ready_directory_plans(
        &self,
        directory: &str,
        plans: Vec<PlannedCatalogWrite>,
    ) -> anyhow::Result<Vec<PlannedCatalogWrite>> {
        let plans = coalesce_overlapping_plans(plans)?;
        let mut remaining = Vec::new();

        for plan in plans {
            let filename = catalog_filename(
                UnixNanos::from(plan.start_ts),
                UnixNanos::from(plan.end_ts),
                Some(&plan.group_identity),
            );
            let path = format!("{directory}/{filename}");
            if !self.file_exists(&path)? {
                remaining.push(plan);
            }
        }

        if remaining.is_empty() {
            return Ok(remaining);
        }

        let existing = self.get_directory_intervals(directory)?;
        let mut intervals = existing.clone();
        intervals.extend(remaining.iter().map(|plan| (plan.start_ts, plan.end_ts)));

        if !are_intervals_disjoint(&intervals) {
            anyhow::bail!(
                "Writing promoted groups for {directory} would create non-disjoint intervals. \
                 Existing intervals: {existing:?}"
            );
        }

        Ok(remaining)
    }

    fn with_catalog_identifier_metadata(
        batch: RecordBatch,
        catalog_data_name: &str,
        identifier: Option<&str>,
    ) -> anyhow::Result<RecordBatch> {
        let Some(identifier) = identifier else {
            return Ok(batch);
        };

        let metadata_key = if catalog_data_name == "bars" {
            "bar_type"
        } else {
            "instrument_id"
        };

        if batch.schema().metadata().contains_key(metadata_key) {
            return Ok(batch);
        }

        let mut metadata = batch.schema().metadata().clone();
        metadata.insert(metadata_key.to_string(), identifier.to_string());

        let schema = Arc::new(Schema::new_with_metadata(
            batch.schema().fields().clone(),
            metadata,
        ));
        Ok(RecordBatch::try_new(schema, batch.columns().to_vec())?)
    }

    fn apply_stream_conversion_transforms(
        batches: &[RecordBatch],
        use_ts_event_for_ts_init: bool,
    ) -> anyhow::Result<Option<RecordBatch>> {
        coalesce_stream_conversion_batches(
            batches,
            StreamConversionOptions {
                use_ts_event_for_ts_init,
                convert_bar_type_to_external: true,
            },
        )
    }

    fn ts_init_range(batch: &RecordBatch) -> anyhow::Result<(u64, u64)> {
        let ts_init = Self::ts_init_array(batch)?;
        if ts_init.is_empty() {
            anyhow::bail!("Cannot convert empty stream batch to parquet");
        }

        if (0..ts_init.len()).any(|row| ts_init.is_null(row)) {
            anyhow::bail!("ts_init column contains null values");
        }

        let start = ts_init
            .value(0)
            .ok_or_else(|| anyhow::anyhow!("ts_init value cannot be negative"))?;
        let end = ts_init
            .value(ts_init.len() - 1)
            .ok_or_else(|| anyhow::anyhow!("ts_init value cannot be negative"))?;
        Ok((start, end))
    }

    fn ts_init_array(batch: &RecordBatch) -> anyhow::Result<U64ColumnRef<'_>> {
        let ts_init_idx = batch
            .schema()
            .index_of("ts_init")
            .map_err(|_| anyhow::anyhow!("ts_init column not found"))?;
        U64ColumnRef::try_from_array(batch.column(ts_init_idx).as_ref())
            .ok_or_else(|| anyhow::anyhow!("ts_init column has an unsupported type"))
    }

    fn identifier_from_batch_or_path(
        batch: &RecordBatch,
        feather_path: &str,
        subdirectory: &str,
        instance_id: &str,
    ) -> Option<String> {
        let metadata = batch.schema().metadata().clone();
        if let Some(bar_type) = metadata.get("bar_type") {
            return Some(bar_type.clone());
        }

        if let Some(instrument_id) = metadata.get("instrument_id") {
            return Some(instrument_id.clone());
        }

        if let Ok(identifiers) = identifiers_from_record_batches(std::slice::from_ref(batch))
            && identifiers.len() == 1
        {
            return identifiers.into_iter().next();
        }

        identifier_from_session_feather_path(feather_path, subdirectory, instance_id)
    }

    fn stream_data_name(data_type: &CatalogDataType) -> anyhow::Result<Cow<'static, str>> {
        // Streams stage instruments under the single aggregate name,
        // with the class carried per batch, so a class selector names no staged directory.
        let stream_data_name = match data_type {
            CatalogDataType::Data(data_type) => parquet_data_path_prefix(data_type),
            CatalogDataType::Record(record_type) => record_path_prefix(record_type),
            CatalogDataType::Instrument(class) => {
                anyhow::bail!(
                    "Streams stage instruments under the aggregate family, not {class}; \
                     pass the Instrument data type"
                );
            }
        };

        if !Self::is_supported_stream_data_type(&stream_data_name) {
            anyhow::bail!("Streams do not support {data_type}");
        }

        Ok(stream_data_name)
    }

    fn canonical_stream_data_name(data_name: &str) -> &str {
        match data_name {
            "quote_tick" => "quotes",
            "trade_tick" => "trades",
            "bar" => "bars",
            "mark_price_update" => "mark_prices",
            "index_price_update" => "index_prices",
            "funding_rate_update" => "funding_rates",
            "instrument_close" => "instrument_closes",
            "order_book_delta" => "order_book_deltas",
            other => other,
        }
    }

    fn is_supported_stream_data_type(data_name: &str) -> bool {
        data_name.starts_with("custom/")
            || matches!(
                data_name,
                "instruments"
                    | "quotes"
                    | "trades"
                    | "order_book_deltas"
                    | "order_book_depths"
                    | "bars"
                    | "index_prices"
                    | "mark_prices"
                    | "option_greeks"
                    | "instrument_status"
                    | "instrument_closes"
                    | "funding_rates"
                    | "account_state"
                    | "order_initialized"
                    | "order_denied"
                    | "order_emulated"
                    | "order_submitted"
                    | "order_accepted"
                    | "order_rejected"
                    | "order_pending_cancel"
                    | "order_canceled"
                    | "order_cancel_rejected"
                    | "order_expired"
                    | "order_triggered"
                    | "order_pending_update"
                    | "order_released"
                    | "order_modify_rejected"
                    | "order_updated"
                    | "order_filled"
                    | "order_fill_voided"
                    | "position_opened"
                    | "position_changed"
                    | "position_closed"
                    | "position_adjusted"
                    | "order_snapshot"
                    | "position_snapshot"
                    | "order_status_report"
                    | "fill_report"
                    | "position_status_report"
                    | "execution_mass_status"
            )
    }
}

/// Reads the sealed Feather files a streaming writer produced for one run, without a catalog.
///
/// `writer_path` is the local streaming writer root (`StreamingConfig.writer_path`). The run's
/// files live under `{writer_path}/{environment}/{instance_id}`.
///
/// `data_types` limits the read to those stream families, `identifiers` to the records whose
/// identifier (instrument ID, bar type, or custom data identifier) contains one of them, and
/// `start`/`end` to the inclusive `ts_init` range. Every data type is read for every matching
/// identifier. Records without an identifier pass the identifier filter. Open
/// `.feather.partial` files are not read.
/// The result is sorted by `ts_init`.
///
/// # Errors
///
/// Returns an error if `writer_path` is not local, a requested data type is not a readable stream
/// family, or listing, reading, or decoding a Feather file fails.
pub fn read_feather_run(
    writer_path: &str,
    environment: Environment,
    instance_id: &str,
    data_types: Option<&[CatalogDataType]>,
    identifiers: Option<&[String]>,
    start: Option<UnixNanos>,
    end: Option<UnixNanos>,
) -> anyhow::Result<Vec<Data>> {
    let root = local_writer_directory(writer_path)?;
    let catalog = ParquetDataCatalog::new(&root, None, None, None, None);
    catalog.read_run(
        environment_directory(environment),
        instance_id,
        data_types,
        identifiers,
        start,
        end,
    )
}

struct PlannedCatalogWrite {
    directory: String,
    start_ts: u64,
    end_ts: u64,
    batch: RecordBatch,
    group_identity: String,
}

fn coalesce_overlapping_plans(
    mut plans: Vec<PlannedCatalogWrite>,
) -> anyhow::Result<Vec<PlannedCatalogWrite>> {
    loop {
        let mut changed = false;
        let mut next = Vec::new();

        while let Some(plan) = plans.pop() {
            if let Some(index) = next
                .iter()
                .position(|other| plan_intervals_overlap(other, &plan))
            {
                let other = next.swap_remove(index);
                next.push(unify_plans(other, plan)?);
                changed = true;
            } else {
                next.push(plan);
            }
        }

        plans = next;

        if !changed {
            break;
        }
    }

    Ok(plans)
}

fn plan_intervals_overlap(left: &PlannedCatalogWrite, right: &PlannedCatalogWrite) -> bool {
    left.start_ts <= right.end_ts && right.start_ts <= left.end_ts
}

fn unify_plans(
    left: PlannedCatalogWrite,
    right: PlannedCatalogWrite,
) -> anyhow::Result<PlannedCatalogWrite> {
    let batch = unify_record_batches(left.batch, right.batch)?;
    let (start_ts, end_ts) = ParquetDataCatalog::ts_init_range(&batch)?;

    Ok(PlannedCatalogWrite {
        directory: left.directory,
        start_ts,
        end_ts,
        batch,
        group_identity: left.group_identity,
    })
}

fn unify_record_batches(left: RecordBatch, right: RecordBatch) -> anyhow::Result<RecordBatch> {
    if left.schema() == right.schema() {
        return concat_sorted(&left, &right);
    }

    anyhow::ensure!(
        left.schema().fields() == right.schema().fields()
            && metadata_without_precision(left.schema().as_ref())
                == metadata_without_precision(right.schema().as_ref()),
        "overlapping promotion groups have incompatible schemas"
    );

    let target = precision_target_schema(left.schema().as_ref(), right.schema().as_ref())?;
    let left = relabel_precision(left, &target)?;
    let right = relabel_precision(right, &target)?;
    concat_sorted(&left, &right)
}

fn precision_target_schema(left: &Schema, right: &Schema) -> anyhow::Result<Schema> {
    if precision_values(left) == precision_values(right) {
        return Ok(left.clone());
    }

    if is_precision_sentinel(left) && !is_precision_sentinel(right) {
        return Ok(right.clone());
    }

    if is_precision_sentinel(right) && !is_precision_sentinel(left) {
        return Ok(left.clone());
    }

    anyhow::bail!("overlapping promotion groups have incompatible precision metadata")
}

fn relabel_precision(batch: RecordBatch, target: &Schema) -> anyhow::Result<RecordBatch> {
    if batch.schema().as_ref() == target {
        return Ok(batch);
    }

    let precision_changes = precision_values(batch.schema().as_ref()) != precision_values(target);
    if precision_changes && batch_has_present_decimal(&batch) {
        anyhow::bail!(
            "cannot relabel precision metadata for a promotion group that contains decimal values"
        );
    }

    Ok(RecordBatch::try_new(
        Arc::new(target.clone()),
        batch.columns().to_vec(),
    )?)
}

fn concat_sorted(left: &RecordBatch, right: &RecordBatch) -> anyhow::Result<RecordBatch> {
    let schema = left.schema();
    let batch = concat_batches(&schema, [left, right])
        .map_err(|e| anyhow::anyhow!("Failed to concatenate promotion groups: {e}"))?;
    sort_by_ts_init(&batch)
}

fn sort_by_ts_init(batch: &RecordBatch) -> anyhow::Result<RecordBatch> {
    let ts_init = batch
        .schema()
        .index_of("ts_init")
        .map_err(|_| anyhow::anyhow!("ts_init column not found"))?;
    let original_row_index = Arc::new(UInt64Array::from_iter_values(0..batch.num_rows() as u64));

    let options = Some(SortOptions {
        descending: false,
        nulls_first: false,
    });

    let indices = lexsort_to_indices(
        &[
            SortColumn {
                values: batch.column(ts_init).clone(),
                options,
            },
            SortColumn {
                values: original_row_index,
                options,
            },
        ],
        None,
    )
    .map_err(|e| anyhow::anyhow!("Failed to sort promotion group: {e}"))?;

    take_record_batch(batch, &indices)
        .map_err(|e| anyhow::anyhow!("Failed to reorder promotion group: {e}"))
}

fn metadata_without_precision(schema: &Schema) -> HashMap<String, String> {
    schema
        .metadata()
        .iter()
        .filter(|(key, _)| *key != "price_precision" && *key != "size_precision")
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect()
}

fn precision_values(schema: &Schema) -> (Option<&str>, Option<&str>) {
    (
        schema.metadata().get("price_precision").map(String::as_str),
        schema.metadata().get("size_precision").map(String::as_str),
    )
}

fn is_precision_sentinel(schema: &Schema) -> bool {
    let (price, size) = precision_values(schema);
    (price.is_some() || size.is_some())
        && price.is_none_or(|value| value == "0")
        && size.is_none_or(|value| value == "0")
}

fn batch_has_present_decimal(batch: &RecordBatch) -> bool {
    batch
        .columns()
        .iter()
        .any(|column| array_has_present_decimal(column.as_ref()))
}

fn array_has_present_decimal(array: &dyn Array) -> bool {
    match array.data_type() {
        DataType::Decimal128(_, _) | DataType::Decimal256(_, _) => {
            !array.is_empty() && array.null_count() < array.len()
        }
        DataType::List(_) => array
            .as_any()
            .downcast_ref::<ListArray>()
            .is_none_or(|list| array_has_present_decimal(list.values().as_ref())),
        DataType::LargeList(_) => array
            .as_any()
            .downcast_ref::<LargeListArray>()
            .is_none_or(|list| array_has_present_decimal(list.values().as_ref())),
        DataType::FixedSizeList(_, _) => array
            .as_any()
            .downcast_ref::<FixedSizeListArray>()
            .is_none_or(|list| array_has_present_decimal(list.values().as_ref())),
        DataType::Struct(_) => array
            .as_any()
            .downcast_ref::<StructArray>()
            .is_none_or(|values| {
                values
                    .columns()
                    .iter()
                    .any(|column| array_has_present_decimal(column.as_ref()))
            }),
        _ => false,
    }
}

#[cfg(test)]
mod promotion_group_tests {
    use std::{collections::HashMap, sync::Arc};

    use datafusion::arrow::{
        array::{Array, Decimal128Array, UInt64Array},
        datatypes::{DataType, Field, Schema},
        record_batch::RecordBatch,
    };
    use nautilus_model::data::NautilusDataType;
    use rstest::rstest;
    use tempfile::TempDir;

    use crate::{
        backend::parquet::{catalog::ParquetDataCatalog, io::read_parquet_from_object_store},
        common::storage::create_storage_backend_from_path,
        writer::{
            feather::{NAUTILUS_ARROW_METADATA_ID_COLUMN, NAUTILUS_ARROW_METADATA_JSON_COLUMN},
            run::FeatherSessionSource,
        },
    };

    fn precision_batch(precision: &str, timestamps: Vec<u64>, price: Option<i128>) -> RecordBatch {
        let mut metadata = HashMap::new();
        metadata.insert("instrument_id".to_string(), "ETH/USDT.BINANCE".to_string());
        metadata.insert("price_precision".to_string(), precision.to_string());
        metadata.insert("size_precision".to_string(), "0".to_string());
        let price = Decimal128Array::from(vec![price; timestamps.len()])
            .with_precision_and_scale(38, 16)
            .unwrap();
        RecordBatch::try_new(
            Arc::new(Schema::new_with_metadata(
                vec![
                    Field::new("ts_init", DataType::UInt64, false),
                    Field::new("price", price.data_type().clone(), true),
                ],
                metadata,
            )),
            vec![Arc::new(UInt64Array::from(timestamps)), Arc::new(price)],
        )
        .unwrap()
    }

    #[rstest]
    fn overlapping_empty_precision_group_is_promoted() {
        let temp = TempDir::new().unwrap();
        let catalog = ParquetDataCatalog::new(temp.path(), None, None, None, None);
        let batches = vec![
            precision_batch("0", vec![1, 3], None),
            precision_batch("2", vec![2], Some(20_000_000_000_000_000)),
        ];

        catalog
            .convert_feather_batches_to_parquet(
                "backtest",
                "run-1",
                "quotes",
                "backtest/run-1/quotes_1.feather",
                &batches,
                false,
                Some("replay"),
            )
            .unwrap();
        catalog
            .convert_feather_batches_to_parquet(
                "backtest",
                "run-1",
                "quotes",
                "backtest/run-1/quotes_1.feather",
                &batches,
                false,
                Some("replay"),
            )
            .unwrap();

        assert_eq!(
            catalog
                .get_intervals(
                    &NautilusDataType::QuoteTick.into(),
                    Some("ETH/USDT.BINANCE")
                )
                .unwrap(),
            vec![(1, 3)],
        );
    }

    #[rstest]
    fn overlapping_incompatible_groups_write_nothing() {
        let temp = TempDir::new().unwrap();
        let catalog = ParquetDataCatalog::new(temp.path(), None, None, None, None);
        let batches = vec![
            precision_batch("2", vec![1, 3], Some(1)),
            precision_batch("5", vec![2], Some(2)),
        ];

        let error = catalog
            .convert_feather_batches_to_parquet(
                "backtest",
                "run-1",
                "quotes",
                "backtest/run-1/quotes_1.feather",
                &batches,
                false,
                Some("replay"),
            )
            .unwrap_err();
        assert_eq!(
            error.to_string(),
            "overlapping promotion groups have incompatible precision metadata"
        );
        assert!(
            catalog
                .query_files(&NautilusDataType::QuoteTick.into(), None, None, None)
                .unwrap()
                .is_empty()
        );
    }

    #[rstest]
    fn overlapping_groups_keep_source_order_for_equal_ts_init() {
        let temp = TempDir::new().unwrap();
        let catalog = ParquetDataCatalog::new(temp.path(), None, None, None, None);
        let batches = vec![
            precision_batch("2", vec![1, 2], Some(10)),
            precision_batch("0", vec![2, 3], None),
        ];

        catalog
            .convert_feather_batches_to_parquet(
                "backtest",
                "run-1",
                "quotes",
                "backtest/run-1/quotes_1.feather",
                &batches,
                false,
                Some("replay"),
            )
            .unwrap();

        let files = catalog
            .query_files(&NautilusDataType::QuoteTick.into(), None, None, None)
            .unwrap();

        let (written, _) = catalog
            .execute_async(|| async {
                read_parquet_from_object_store(
                    catalog.object_store.clone(),
                    &object_store::path::Path::from(files[0].as_str()),
                )
                .await
            })
            .unwrap();

        let rows = written
            .iter()
            .flat_map(|batch| {
                let ts_init = batch
                    .column_by_name("ts_init")
                    .unwrap()
                    .as_any()
                    .downcast_ref::<UInt64Array>()
                    .unwrap()
                    .values()
                    .to_vec();
                let price = batch
                    .column_by_name("price")
                    .unwrap()
                    .as_any()
                    .downcast_ref::<Decimal128Array>()
                    .unwrap()
                    .iter()
                    .collect::<Vec<_>>();
                ts_init.into_iter().zip(price)
            })
            .collect::<Vec<_>>();

        // Coalescing unifies the later group as the left side, so its rows lead on equal ts_init
        assert_eq!(files.len(), 1);
        assert_eq!(
            rows,
            vec![(1, Some(10)), (2, None), (2, Some(10)), (3, None)]
        );
    }

    #[rstest]
    fn promote_feather_file_skips_staged_batches_without_rows() {
        let temp = TempDir::new().unwrap();
        let catalog = ParquetDataCatalog::new(temp.path(), None, None, None, None);
        let storage =
            create_storage_backend_from_path(temp.path().to_str().unwrap(), None).unwrap();
        let source = FeatherSessionSource::new(storage, "backtest", "run-1");
        let staged = RecordBatch::new_empty(Arc::new(Schema::new(vec![
            Field::new("ts_init", DataType::UInt64, false),
            Field::new(NAUTILUS_ARROW_METADATA_ID_COLUMN, DataType::Utf8, false),
            Field::new(NAUTILUS_ARROW_METADATA_JSON_COLUMN, DataType::Utf8, false),
        ])));

        let summary = catalog
            .promote_feather_file(
                &source,
                "backtest/run-1/quotes_1.feather",
                vec![staged],
                false,
                "replay",
            )
            .unwrap();

        assert!(summary.is_none());
        assert!(
            catalog
                .query_files(&NautilusDataType::QuoteTick.into(), None, None, None)
                .unwrap()
                .is_empty()
        );
    }

    #[rstest]
    fn overlapping_groups_with_different_fields_write_nothing() {
        let temp = TempDir::new().unwrap();
        let catalog = ParquetDataCatalog::new(temp.path(), None, None, None, None);
        let nullable = precision_batch("2", vec![2], Some(2));
        let schema = nullable.schema();
        let non_nullable = RecordBatch::try_new(
            Arc::new(Schema::new_with_metadata(
                vec![
                    schema.field(0).clone(),
                    schema.field(1).clone().with_nullable(false),
                ],
                schema.metadata().clone(),
            )),
            nullable.columns().to_vec(),
        )
        .unwrap();
        let batches = vec![precision_batch("2", vec![1, 3], Some(1)), non_nullable];

        let error = catalog
            .convert_feather_batches_to_parquet(
                "backtest",
                "run-1",
                "quotes",
                "backtest/run-1/quotes_1.feather",
                &batches,
                false,
                Some("replay"),
            )
            .unwrap_err();

        assert_eq!(
            error.to_string(),
            "overlapping promotion groups have incompatible schemas"
        );
        assert!(
            catalog
                .query_files(&NautilusDataType::QuoteTick.into(), None, None, None)
                .unwrap()
                .is_empty()
        );
    }
}

#[cfg(test)]
mod canonical_name_tests {
    use rstest::rstest;

    use super::ParquetDataCatalog;

    #[rstest]
    #[case("quotes", "quotes")]
    #[case("trades", "trades")]
    #[case("bars", "bars")]
    #[case("order_book_delta", "order_book_deltas")]
    #[case("mark_prices", "mark_prices")]
    #[case("index_prices", "index_prices")]
    #[case("funding_rates", "funding_rates")]
    #[case("instrument_closes", "instrument_closes")]
    #[case("quotes", "quotes")]
    fn canonical_stream_data_aliases(#[case] input: &str, #[case] expected: &str) {
        assert_eq!(
            ParquetDataCatalog::canonical_stream_data_name(input),
            expected,
        );
    }
}

#[cfg(test)]
mod read_run_tests {
    use std::{
        path::Path,
        sync::{Arc, atomic::AtomicU64},
    };

    use nautilus_common::enums::Environment;
    use nautilus_core::UnixNanos;
    use nautilus_model::data::{
        Data, NautilusDataType, NautilusRecordType, QuoteTick, TradeTick,
        stubs::{quote_audusd, quote_ethusdt_binance, stub_trade_ethusdt_buy},
    };
    use rstest::rstest;
    use tempfile::TempDir;

    use super::read_feather_run;
    use crate::{
        backend::default_writer_factories,
        catalog::{factory::CatalogConnectConfig, types::CatalogDataType},
        writer::{
            factory::{WriterBackendType, WriterConnectConfig, create_writer},
            feather::{FeatherWriter, RotationConfig, WriterClock},
        },
    };

    fn quote_btc(ts: u64) -> QuoteTick {
        QuoteTick {
            instrument_id: "BTCUSDT-PERP.BINANCE".into(),
            ts_event: UnixNanos::from(ts),
            ts_init: UnixNanos::from(ts),
            ..quote_ethusdt_binance()
        }
    }

    fn quote_aud(ts: u64) -> QuoteTick {
        QuoteTick {
            ts_event: UnixNanos::from(ts),
            ts_init: UnixNanos::from(ts),
            ..quote_audusd()
        }
    }

    fn quote_eth(ts: u64) -> QuoteTick {
        QuoteTick {
            ts_event: UnixNanos::from(ts),
            ts_init: UnixNanos::from(ts),
            ..quote_ethusdt_binance()
        }
    }

    fn trade_eth(ts: u64) -> TradeTick {
        TradeTick {
            ts_event: UnixNanos::from(ts),
            ts_init: UnixNanos::from(ts),
            ..stub_trade_ethusdt_buy()
        }
    }

    fn write_run(writer_path: &Path, data: Vec<Data>) {
        let mut writer = FeatherWriter::new(
            writer_path.join("backtest").join("run-001"),
            WriterClock::Test(Arc::new(AtomicU64::new(0))),
            RotationConfig::NoRotation,
            None,
            Some(FeatherWriter::default_per_instrument_types()),
            None,
        );

        for item in data {
            writer.write_data(item).unwrap();
        }
        writer.close().unwrap();
    }

    fn written_run() -> TempDir {
        let temp_dir = TempDir::new().unwrap();
        write_run(
            temp_dir.path(),
            vec![
                Data::Quote(quote_eth(3_000)),
                Data::Quote(quote_aud(1_000)),
                Data::Trade(trade_eth(2_000)),
                Data::Quote(quote_aud(4_000)),
                Data::Quote(quote_btc(5_000)),
            ],
        );
        temp_dir
    }

    #[rstest]
    fn read_feather_run_returns_every_record_sorted_by_ts_init() {
        let temp_dir = written_run();

        let data = read_feather_run(
            temp_dir.path().to_str().unwrap(),
            Environment::Backtest,
            "run-001",
            None,
            None,
            None,
            None,
        )
        .unwrap();

        assert_eq!(
            data,
            vec![
                Data::Quote(quote_aud(1_000)),
                Data::Trade(trade_eth(2_000)),
                Data::Quote(quote_eth(3_000)),
                Data::Quote(quote_aud(4_000)),
                Data::Quote(quote_btc(5_000)),
            ],
        );
    }

    #[rstest]
    #[case::data_type(
        Some(vec![CatalogDataType::from(NautilusDataType::TradeTick)]),
        None,
        None,
        None,
        vec![Data::Trade(trade_eth(2_000))],
    )]
    #[case::identifier(
        None,
        Some(vec!["AUD/USD.SIM".to_string()]),
        None,
        None,
        vec![Data::Quote(quote_aud(1_000)), Data::Quote(quote_aud(4_000))],
    )]
    #[case::inclusive_range(
        None,
        None,
        Some(2_000),
        Some(3_000),
        vec![Data::Trade(trade_eth(2_000)), Data::Quote(quote_eth(3_000))],
    )]
    #[case::types_by_identifiers(
        Some(vec![
            CatalogDataType::from(NautilusDataType::QuoteTick),
            CatalogDataType::from(NautilusDataType::TradeTick),
        ]),
        Some(vec!["AUD/USD.SIM".to_string(), "ETHUSDT-PERP.BINANCE".to_string()]),
        None,
        None,
        vec![
            Data::Quote(quote_aud(1_000)),
            Data::Trade(trade_eth(2_000)),
            Data::Quote(quote_eth(3_000)),
            Data::Quote(quote_aud(4_000)),
        ],
    )]
    #[case::combined(
        Some(vec![CatalogDataType::from(NautilusDataType::QuoteTick)]),
        Some(vec!["ETHUSDT-PERP.BINANCE".to_string()]),
        Some(1_000),
        None,
        vec![Data::Quote(quote_eth(3_000))],
    )]
    fn read_feather_run_applies_filters(
        #[case] data_types: Option<Vec<CatalogDataType>>,
        #[case] identifiers: Option<Vec<String>>,
        #[case] start: Option<u64>,
        #[case] end: Option<u64>,
        #[case] expected: Vec<Data>,
    ) {
        let temp_dir = written_run();

        let data = read_feather_run(
            temp_dir.path().to_str().unwrap(),
            Environment::Backtest,
            "run-001",
            data_types.as_deref(),
            identifiers.as_deref(),
            start.map(UnixNanos::from),
            end.map(UnixNanos::from),
        )
        .unwrap();

        assert_eq!(data, expected);
    }

    // The standalone Feather writer stages a file per identifier, while the Parquet writer stages
    // one file per type with an identifier column, so the filters must select rows in both layouts
    #[rstest]
    #[case::feather(WriterBackendType::Feather, false)]
    #[case::parquet(WriterBackendType::Parquet, true)]
    fn read_feather_run_filters_every_writer_layout(
        #[case] backend: WriterBackendType,
        #[case] needs_catalog: bool,
    ) {
        let temp_dir = TempDir::new().unwrap();
        let writer_path = temp_dir.path().join("stream");
        let catalog = needs_catalog.then(|| {
            CatalogConnectConfig::new(temp_dir.path().join("catalog").to_str().unwrap(), None)
        });
        let mut config = WriterConnectConfig::new(
            writer_path
                .join("backtest")
                .join("run-001")
                .to_str()
                .unwrap(),
            catalog,
        );
        config.promote_on_close = false;
        let mut writer = create_writer(
            &backend,
            &config,
            WriterClock::Test(Arc::new(AtomicU64::new(0))),
            &default_writer_factories(),
        )
        .unwrap();

        for data in [
            Data::Quote(quote_eth(3_000)),
            Data::Quote(quote_aud(1_000)),
            Data::Trade(trade_eth(2_000)),
            Data::Quote(quote_aud(4_000)),
            Data::Quote(quote_btc(2_500)),
        ] {
            writer.write_data(data).unwrap();
        }
        writer.close().unwrap();
        let data_types = [
            CatalogDataType::from(NautilusDataType::QuoteTick),
            CatalogDataType::from(NautilusDataType::TradeTick),
        ];
        let identifiers = [
            "AUD/USD.SIM".to_string(),
            "ETHUSDT-PERP.BINANCE".to_string(),
        ];

        let data = read_feather_run(
            writer_path.to_str().unwrap(),
            Environment::Backtest,
            "run-001",
            Some(&data_types),
            Some(&identifiers),
            None,
            Some(UnixNanos::from(3_000)),
        )
        .unwrap();

        assert_eq!(
            data,
            vec![
                Data::Quote(quote_aud(1_000)),
                Data::Trade(trade_eth(2_000)),
                Data::Quote(quote_eth(3_000)),
            ],
        );
    }

    #[rstest]
    fn read_feather_run_skips_partial_files() {
        let temp_dir = TempDir::new().unwrap();
        let mut writer = FeatherWriter::new(
            temp_dir.path().join("backtest").join("run-001"),
            WriterClock::Test(Arc::new(AtomicU64::new(0))),
            RotationConfig::NoRotation,
            None,
            Some(FeatherWriter::default_per_instrument_types()),
            None,
        );
        writer.write_data(Data::Quote(quote_aud(1_000))).unwrap();
        writer.flush().unwrap();

        let data = read_feather_run(
            temp_dir.path().to_str().unwrap(),
            Environment::Backtest,
            "run-001",
            None,
            None,
            None,
            None,
        )
        .unwrap();

        assert_eq!(data, Vec::<Data>::new());
        writer.close().unwrap();
    }

    #[rstest]
    fn read_feather_run_rejects_requested_family_without_data_decoding() {
        let temp_dir = written_run();
        let data_types = [CatalogDataType::from(NautilusRecordType::AccountState)];

        let error = read_feather_run(
            temp_dir.path().to_str().unwrap(),
            Environment::Backtest,
            "run-001",
            Some(&data_types),
            None,
            None,
            None,
        )
        .unwrap_err();

        assert_eq!(
            error.to_string(),
            "Cannot read AccountState from a Feather run as data",
        );
    }

    #[rstest]
    fn read_feather_run_rejects_remote_writer_path() {
        let error = read_feather_run(
            "s3://bucket/stream",
            Environment::Backtest,
            "run-001",
            None,
            None,
            None,
            None,
        )
        .unwrap_err();

        assert_eq!(
            error.to_string(),
            "Streaming writers append to local files, writer path must be local, was s3://bucket/stream",
        );
    }

    #[rstest]
    #[cfg(feature = "python")]
    fn read_feather_run_reads_custom_data() {
        use nautilus_model::{
            data::{CustomData, DataType},
            identifiers::InstrumentId,
        };
        use nautilus_serialization::ensure_custom_data_registered;

        use crate::test_data::RustTestCustomData;

        ensure_custom_data_registered::<RustTestCustomData>();
        let instrument_id = InstrumentId::from("RUST.TEST");
        let custom = Data::Custom(CustomData::new(
            Arc::new(RustTestCustomData {
                instrument_id,
                value: 1.23,
                flag: true,
                ts_event: UnixNanos::from(5_000),
                ts_init: UnixNanos::from(5_000),
            }),
            DataType::new("RustTestCustomData", None, Some(instrument_id.to_string())),
        ));
        let temp_dir = TempDir::new().unwrap();
        write_run(
            temp_dir.path(),
            vec![Data::Quote(quote_aud(1_000)), custom.clone()],
        );

        let data = read_feather_run(
            temp_dir.path().to_str().unwrap(),
            Environment::Backtest,
            "run-001",
            None,
            None,
            None,
            None,
        )
        .unwrap();

        assert_eq!(data, vec![Data::Quote(quote_aud(1_000)), custom]);
    }
}
