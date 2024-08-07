//! This module contains the implementation of the `read` CLI command

use std::{
    collections::BTreeMap,
    io::{self, ErrorKind},
    path::{Path, PathBuf},
};

use anyhow::Context;
use argh::FromArgs;

use crate::{
    archive::read_archive_value,
    staging::StagingFileReader,
    value::{merge::MergeSettings, Value},
};

/// The `read` sub-command reads and merges all the archived JSON data
/// into a single object and outputs it to stdout.
#[derive(Debug, PartialEq, FromArgs)]
#[argh(subcommand, name = "read")]
pub struct ReadCommand {}

impl ReadCommand {
    /// This function executes the read command.
    #[tracing::instrument]
    pub fn execute(self, data_dir: PathBuf) -> anyhow::Result<()> {
        let mut scratch_buffer = Vec::<u8>::new();

        let archived_value = collect_archived_values(&mut scratch_buffer, &data_dir)
            .context("collecting and merging all archived values")?;

        let staging_value = StagingFileReader::read_merged_value(&data_dir)
            .context("opening staging file for archiving")?;

        let final_value = match (archived_value, staging_value) {
            (None, None) => {
                tracing::warn!("No data is present in archive or staging");
                return Ok(());
            }
            (None, Some(value)) | (Some(value), None) => value,
            (Some(accum), Some(value)) => {
                let merge_settings = MergeSettings::default();

                merge_settings.merge(accum, value)
            }
        };

        let stdout = io::stdout();
        let handle = stdout.lock();

        serde_json::to_writer(handle, &final_value).context("writing final value to stdout")?;

        Ok(())
    }
}

fn collect_archived_values(
    scratch_buffer: &mut Vec<u8>,
    data_dir: &Path,
) -> anyhow::Result<Option<Value>> {
    let archive_dir_entries = match data_dir.join("archived").read_dir() {
        Ok(entries) => entries,
        Err(err) => {
            if matches!(err.kind(), ErrorKind::NotFound) {
                // archived directory does not exist
                return Ok(None);
            } else {
                return Err(err).context("reading archived directory entries");
            }
        }
    };

    // Iterate through all dir entries ordered by filename (the timestamp part of the filename specifically)
    let mut all_entries = archive_dir_entries
        .map(|res| res.map(|entry| (entry.file_name(), entry)))
        .collect::<Result<BTreeMap<_, _>, _>>()
        .context("reading all dir entries into set")?;

    let Some((_, first_entry)) = all_entries.pop_first() else {
        // The directory was empty
        return Ok(None);
    };

    let mut accum = read_archive_value(&first_entry.path(), scratch_buffer)
        .context("reading first archive value")?;

    let merge_settings = MergeSettings::default();

    for (_, entry) in all_entries {
        scratch_buffer.clear();

        let value =
            read_archive_value(&entry.path(), scratch_buffer).context("reading archive value")?;

        accum = merge_settings.merge(accum, value);
    }

    Ok(Some(accum))
}
