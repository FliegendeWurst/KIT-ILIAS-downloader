use anyhow::Context;
use tokio::fs::File as AsyncFile;
use tokio::io::{AsyncRead, BufWriter};

use std::path::Path;

use super::Result;

pub async fn write_file_data<R: ?Sized>(path: &Path, data: &mut R) -> Result<()> 
where R: AsyncRead + Unpin {
	let file = AsyncFile::create(&path).await.context("failed to create file")?;
	let mut file = BufWriter::new(file);
	tokio::io::copy(data, &mut file).await.context("failed to write to file")?;
	Ok(())
}