//! This module contains things relating to reading and writing to archive file

use std::{
    fs::{self, OpenOptions},
    io::{BufRead, BufReader, BufWriter, Read, Seek, SeekFrom, Write},
    path::Path,
};

use anyhow::Context;
use crc32fast::Hasher;
use jiff::{fmt::temporal::DateTimePrinter, Timestamp};
use zerocopy::{AsBytes, FromBytes, FromZeroes, Unaligned};

use crate::value::Value;

/// TODO
pub fn read_archive_value(
    archive_path: &Path,
    scratch_buffer: &mut Vec<u8>,
) -> anyhow::Result<Value> {
    let start_index = scratch_buffer.len();

    let archive_file = OpenOptions::new()
        .read(true)
        .open(archive_path)
        .context("opening archive file for reading")?;

    let mut reader = ArchiveReader::new(archive_file).context("starting to read archive")?;

    reader
        .read_to_end(scratch_buffer)
        .context("reading content of archive file")?;

    let body = &scratch_buffer[start_index..];

    reader.metadata.assert_checksum(body)?;
    let mut cbor_reader = minicbor::Decoder::new(body);
    let value = cbor_reader.decode().context("decoding CBOR value")?;

    Ok(value)
}

/// Write a new archive file to the given data directory, with the content of
/// the given CBOR value.
#[tracing::instrument(skip_all)]
pub fn write_archive_value(data_dir: &Path, value: Value) -> anyhow::Result<()> {
    // 2024-06-19-19:22:45Z
    let mut now = String::with_capacity(20);
    DateTimePrinter::new()
        .separator(b'-')
        .print_timestamp(&Timestamp::now(), &mut now)
        .context("formatting now for archive filename")?;
    // 2024-06-19-19-22-45
    now = now.replace(':', "-").replace('Z', "");
    let archive_file_path = data_dir.join(format!("archived/{now}.bin"));

    fs::create_dir_all(
        archive_file_path
            .parent()
            .expect("path created with parent"),
    )
    .context("creating 'archived' folder if not present")?;

    // Choosing to ignore AlreadyExists errors, it should be retried by the caller
    // TODO: Could improve this by adding a `.{counter}` to the filename, but
    // its a bit annoying
    tracing::debug!(archive_file = %archive_file_path.display(), "Creating new archive file");
    let archive_file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&archive_file_path)
        .context("creating new archive file")?;

    // Create the writer and it will handle writing and updating the metadata
    let writer = ArchiveWriter::new(archive_file).context("creating archive file writer")?;

    // Add the CBOR value content
    let mut cbor_writer = minicbor::encode::write::Writer::new(writer);
    minicbor::encode(value, &mut cbor_writer).context("writing CBOR value")?;

    // Close out the metadata, write the checksum, flush the file
    cbor_writer
        .into_inner()
        .finish()
        .context("finishing file and writing metadata")?;

    tracing::debug!(archive_file = %archive_file_path.display(), "Completed writing archive file");

    Ok(())
}

const VERSION: [u8; 4] = u32::to_be_bytes(1);
// WALL•A
const MAGIC: [u8; 8] = *b"WALL\xE2\x80\xA2A";

/// This struct contains metadata used to protect the archive file integrity.
#[derive(Debug, FromZeroes, FromBytes, Unaligned, AsBytes, PartialEq, Eq, Hash)]
#[repr(C)]
struct Metadata {
    magic: [u8; 8],
    version: [u8; 4],
    checksum: [u8; 4],
}

impl Metadata {
    fn from_reader(mut reader: impl BufRead) -> anyhow::Result<Self> {
        let mut buf = Metadata::default();
        reader
            .read_exact(buf.as_bytes_mut())
            .context("trying to read metadata")?;

        Ok(buf)
    }

    fn for_checksum(checksum: u32) -> Self {
        Self {
            magic: MAGIC,
            version: VERSION,
            checksum: checksum.to_be_bytes(),
        }
    }

    /// Create a new metadata based on the content of the given archive body.
    #[cfg(test)]
    fn for_body(body: &[u8]) -> Self {
        Self::for_checksum(crc32fast::hash(body))
    }

    /// Returns `Ok(())` if the given archive body matches the checksum in this metadata.
    ///
    /// Otherwise it returns an error with a custom message about the checksum mismatch.
    fn assert_checksum(&self, body: &[u8]) -> anyhow::Result<()> {
        let checksum = crc32fast::hash(body).to_be_bytes();

        if self.checksum != checksum {
            Err(anyhow::anyhow!(
                "Checksum for given body [{:08x}] did not match checksum from the file metadata [{:08x}]",
                u32::from_be_bytes(checksum),
                u32::from_be_bytes(self.checksum),
            ))
        } else {
            Ok(())
        }
    }

    /// Return true if the given archive body matches the checksum in this metadata.
    #[cfg(test)]
    fn matches_body(&self, body: &[u8]) -> bool {
        let checksum = crc32fast::hash(body).to_be_bytes();
        self.checksum == checksum
    }
}

impl Default for Metadata {
    fn default() -> Self {
        Self {
            magic: MAGIC,
            version: VERSION,
            checksum: [0; 4],
        }
    }
}

#[derive(Debug)]
struct ArchiveWriter<W: Write> {
    start_position: u64,
    hasher: Hasher,
    inner: BufWriter<W>,
}

impl<W: Write> Write for ArchiveWriter<W> {
    fn write(&mut self, buf: &[u8]) -> Result<usize, std::io::Error> {
        self.hasher.update(buf);
        self.inner.write(buf)
    }

    fn flush(&mut self) -> Result<(), std::io::Error> {
        self.inner.flush()
    }
}

impl<W: Write + Seek> ArchiveWriter<W> {
    /// Write a new value archive to the given writer, starting by writing an
    /// empty version of the file metadata.
    fn new(mut writer: W) -> Result<Self, std::io::Error> {
        let start_position = writer.stream_position()?;
        let mut inner = BufWriter::new(writer);
        // Write a dummy metadata to the start of the file, we'll overwrite this
        // in the `finish` method.
        inner.write_all(Metadata::default().as_bytes())?;
        Ok(Self {
            inner,
            hasher: Hasher::new(),
            start_position,
        })
    }

    /// Finish this archive file by finalizing the CRC32 checksum, writing the
    /// full metadata again, and flushing the buffers to the file.
    fn finish(mut self) -> Result<(), std::io::Error> {
        // Rewind to the position where we recorded the metadata the first time
        self.inner.seek(SeekFrom::Start(self.start_position))?;

        let metadata = Metadata::for_checksum(self.hasher.finalize());
        self.inner.write_all(metadata.as_bytes())?;

        Ok(())
    }
}

#[derive(Debug)]
struct ArchiveReader<R> {
    metadata: Metadata,
    inner: BufReader<R>,
}

impl<R: Read> Read for ArchiveReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.inner.read(buf)
    }
}

impl<R: Read> BufRead for ArchiveReader<R> {
    fn fill_buf(&mut self) -> std::io::Result<&[u8]> {
        self.inner.fill_buf()
    }

    fn consume(&mut self, amt: usize) {
        self.inner.consume(amt)
    }
}

impl<R: Read> ArchiveReader<R> {
    fn new(reader: R) -> anyhow::Result<Self> {
        let mut inner = BufReader::new(reader);
        let metadata = Metadata::from_reader(&mut inner)?;

        Ok(Self { metadata, inner })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_metadata() {
        let md = Metadata::for_body(b"klasjdhfaklsdh asdklfjhasldk aldkfjhaskdfjh");
        assert_eq!(md.checksum, [191, 106, 231, 136]);
        assert_eq!(md.magic, MAGIC);
        assert_eq!(md.version, VERSION);

        assert_eq!(
            Metadata::for_body(b"hello sun goodbye moon").checksum,
            [204, 119, 81, 28]
        );
        assert_eq!(
            Metadata::for_body(b"hello moon goodbye sun").checksum,
            [4, 104, 210, 191]
        );
        assert_eq!(
            Metadata::for_body(b"hello mo0n goodbye sun").checksum,
            [117, 247, 173, 212]
        );
        assert_eq!(Metadata::for_body(b"").checksum, [0, 0, 0, 0]);
    }

    #[test]
    fn metadata_body_matches() {
        let md = Metadata::for_body(b"klasjdhfaklsdh asdklfjhasldk aldkfjhaskdfjh");
        assert!(md.matches_body(b"klasjdhfaklsdh asdklfjhasldk aldkfjhaskdfjh"));

        assert!(!md.matches_body(b"klasjdhfaklsdh asdk1fjhasldk aldkfjhaskdfjh"));
        assert!(!md.matches_body(b""));
        assert!(!md.matches_body(b"klasjdhfaklsdh asdklfjhasldk aldkfjhaskdfjh1"));
    }

    #[test]
    fn metadata_as_bytes() {
        let md = Metadata::for_body(b"klasjdhfaklsdh asdklfjhasldk aldkfjhaskdfjh");

        let md_bytes = md.as_bytes();
        assert_eq!(md_bytes.len(), 16);
        assert_eq!(&md_bytes[..8], b"WALL\xE2\x80\xA2A");
        assert_eq!(&md_bytes[8..12], &[0, 0, 0, 1]);
        assert_eq!(&md_bytes[12..16], &[191, 106, 231, 136]);

        let md = Metadata::for_body(b"");

        let md_bytes = md.as_bytes();
        assert_eq!(md_bytes.len(), 16);
        assert_eq!(&md_bytes[..8], b"WALL\xE2\x80\xA2A");
        assert_eq!(&md_bytes[8..12], &[0, 0, 0, 1]);
        assert_eq!(&md_bytes[12..16], &[0, 0, 0, 0]);
    }

    #[test]
    fn metadata_from_bytes() {
        let md = Metadata::read_from(b"WALL\xE2\x80\xA2A\x00\x00\x00\x01\x00\x00\x00\x00").unwrap();
        assert_eq!(md.magic, MAGIC);
        assert_eq!(md.version, VERSION);
        assert_eq!(md.checksum, [0, 0, 0, 0]);
        assert!(md.matches_body(b""));

        let md = Metadata::read_from(b"WALL\xE2\x80\xA2A\x00\x00\x00\x01\xBF\x6A\xE7\x88").unwrap();
        assert_eq!(md.magic, MAGIC);
        assert_eq!(md.version, VERSION);
        assert_eq!(md.checksum, [191, 106, 231, 136]);
        assert!(md.matches_body(b"klasjdhfaklsdh asdklfjhasldk aldkfjhaskdfjh"));
    }
}
