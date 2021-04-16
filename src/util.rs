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

/// Create a directory. Does not error if the directory already exists.
pub async fn create_dir(path: &Path) -> Result<()> {
	if let Err(e) = tokio::fs::create_dir(&path).await {
		if e.kind() != tokio::io::ErrorKind::AlreadyExists {
			return Err(e.into());
		}
	}
	Ok(())
}

// remove once result_flattening is stable (https://github.com/rust-lang/rust/issues/70142)
pub trait Result2 {
	type V;
	type E;
	type F;

	fn flatten2(self) -> Result<Self::V, Self::E>
	where
		Self::F: Into<Self::E>;

	fn flatten_with<O: FnOnce(Self::F) -> Self::E>(self, op: O) -> Result<Self::V, Self::E>;
}

impl<V, E, F> Result2 for Result<Result<V, F>, E> {
	type V = V;
	type E = E;
	type F = F;

	fn flatten2(self) -> Result<Self::V, Self::E>
	where
		Self::F: Into<Self::E>,
	{
		self.flatten_with(|e| e.into())
	}

	fn flatten_with<O: FnOnce(Self::F) -> Self::E>(self, op: O) -> Result<Self::V, Self::E> {
		match self {
			Ok(Ok(v)) => Ok(v),
			Ok(Err(f)) => Err(op(f)),
			Err(e) => Err(e),
		}
	}
}
