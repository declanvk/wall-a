//! This module contains things relating to reading and writing from the staging file

use std::{
    fs::{self, File, Metadata, OpenOptions},
    io::{BufRead, BufReader, BufWriter, Write},
    path::{Path, PathBuf},
};

use crate::value::Value;
use anyhow::Context;

use super::value::merge::MergeSettings;

fn staging_file_path(data_dir: &Path) -> PathBuf {
    data_dir.join("staging.jsonl")
}

/// Delete the staging file
pub fn delete_staging_file(data_dir: &Path) -> anyhow::Result<()> {
    let staging_file_path = staging_file_path(data_dir);

    Ok(fs::remove_file(&staging_file_path)?)
}

/// This struct controls appending to the staging file
#[derive(Debug)]
pub struct StagingFileWriter {
    inner: BufWriter<File>,
    metadata: Metadata,
}

impl StagingFileWriter {
    /// If the given file is not `None`, then flush the buffered writes to
    /// the staging file.
    pub fn flush_if_present(file: &mut Option<Self>) -> anyhow::Result<()> {
        if let Some(ref mut file) = file {
            file.writer().flush().context("flushing staging file")?;
        }

        Ok(())
    }

    /// If the given file is not `None`, open the staging file for appending
    /// data.
    pub fn get_mut_or_open<'f>(
        file: &'f mut Option<Self>,
        data_dir: &Path,
    ) -> anyhow::Result<&'f mut Self> {
        if file.is_none() {
            *file = Some(Self::open(data_dir)?);
        }

        Ok(file.as_mut().unwrap())
    }

    fn open(data_dir: &Path) -> anyhow::Result<Self> {
        let staging_file_path = staging_file_path(data_dir);

        let inner = OpenOptions::new()
            .append(true)
            .create(true)
            .open(staging_file_path)
            .context("opening staging file for writing")?;
        let metadata = inner.metadata().context("reading staging file metadata")?;
        let inner = BufWriter::new(inner);

        Ok(Self { inner, metadata })
    }

    /// Access the underlying [`Writer`] implementation for the staging file.
    pub fn writer(&mut self) -> &mut impl Write {
        &mut self.inner
    }

    /// Return the length in bytes of the staging file when it was first opened.
    pub fn initial_len(&self) -> u64 {
        self.metadata.len()
    }
}

/// This struct controls reading the contents of the staging file
#[derive(Debug)]
pub struct StagingFileReader {
    inner: BufReader<File>,
}

impl StagingFileReader {
    fn open(data_dir: &Path) -> anyhow::Result<Self> {
        let staging_file_path = staging_file_path(data_dir);

        tracing::debug!(
            staging_file = %staging_file_path.display(),
            "Opening staging file for reading"
        );
        let inner = OpenOptions::new()
            .read(true)
            .open(staging_file_path)
            .context("opening staging file for reading")?;
        let inner = BufReader::new(inner);

        Ok(Self { inner })
    }

    /// Open the staging file, read all the lines, and merge those JSON values together.
    ///
    /// Returns `Ok(None)` if the staging file is empty.
    pub fn read_merged_value(data_dir: &Path) -> anyhow::Result<Option<Value<'static>>> {
        let reader = Self::open(data_dir)?;
        let merge_settings = MergeSettings::default();

        let mut accum = None;
        for line in reader.inner.lines() {
            let line = line.context("reading line from staging file")?;
            let value: Value<'_> =
                serde_json::from_str(&line).context("parsing JSON value from staging line")?;

            let value = value.into_owned();

            if let Some(inner_accum) = accum.take() {
                let merged = merge_settings.merge(inner_accum, value);

                accum = Some(merged);
            } else {
                accum = Some(value);
            }
        }
        tracing::trace!(?accum, "Collected merge JSON value from staging file");

        Ok(accum)
    }
}
