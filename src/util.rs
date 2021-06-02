// SPDX-License-Identifier: GPL-3.0-or-later

use anyhow::Context;
use bytes::Bytes;
use futures::TryStreamExt;
use tokio::fs::File as AsyncFile;
use tokio::io::{AsyncRead, BufWriter};
use tokio_util::io::StreamReader;

use std::io;
use std::path::Path;

use crate::Result;

pub async fn write_stream_to_file(
	path: &Path,
	stream: impl futures::Stream<Item = Result<Bytes, reqwest::Error>> + Unpin,
) -> Result<()> {
	let mut reader = StreamReader::new(stream.map_err(|x| io::Error::new(io::ErrorKind::Other, x)));
	write_file_data(&path, &mut reader).await?;
	Ok(())
}

/// Write all data to the specified path. Will overwrite previous file data.
pub async fn write_file_data<R: ?Sized>(path: impl AsRef<Path>, data: &mut R) -> Result<()>
where
	R: AsyncRead + Unpin,
{
	let file = AsyncFile::create(path.as_ref())
		.await
		.context("failed to create file")?;
	let mut file = BufWriter::new(file);
	tokio::io::copy(data, &mut file)
		.await
		.context("failed to write to file")?;
	Ok(())
}

/// Create a directory. Does not error if the directory already exists.
pub async fn create_dir(path: &Path) -> Result<()> {
	if let Err(e) = tokio::fs::create_dir(&path).await {
		if e.kind() != tokio::io::ErrorKind::AlreadyExists {
			return Err(e.into());
		}
	}
	Ok(())
}

#[cfg(not(target_os = "windows"))]
const INVALID: &[char] = &['/', '\\'];
#[cfg(target_os = "windows")]
const INVALID: &[char] = &['/', '\\', ':', '<', '>', '"', '|', '?', '*'];

pub fn file_escape(s: &str) -> String {
	s.replace(INVALID, "-")
}
