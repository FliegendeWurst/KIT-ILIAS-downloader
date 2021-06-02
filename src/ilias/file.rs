use std::{path::Path, sync::Arc};

use anyhow::Result;
use tokio::fs;

use crate::util::write_stream_to_file;

use super::{ILIAS, URL};

pub async fn download(path: &Path, relative_path: &Path, ilias: Arc<ILIAS>, url: &URL) -> Result<()> {
	if ilias.opt.skip_files {
		return Ok(());
	}
	if !ilias.opt.force && fs::metadata(&path).await.is_ok() {
		log!(2, "Skipping download, file exists already");
		return Ok(());
	}
	let data = ilias.download(&url.url).await?;
	log!(0, "Writing {}", relative_path.to_string_lossy());
	write_stream_to_file(&path, data.bytes_stream()).await?;
	Ok(())
}
