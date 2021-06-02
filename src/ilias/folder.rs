use std::{path::Path, sync::Arc};

use anyhow::{Context, Result};

use crate::{
	process_gracefully,
	queue::spawn,
	util::{file_escape, write_file_data},
};

use super::{ILIAS, URL};

pub async fn download(path: &Path, ilias: Arc<ILIAS>, url: &URL) -> Result<()> {
	let content = ilias.get_course_content(&url).await?;
	if let Some(s) = content.1.as_ref() {
		let path = path.join("folder.html");
		write_file_data(&path, &mut s.as_bytes())
			.await
			.context("failed to write folder page html")?;
	}
	for item in content.0 {
		let item = item?;
		let path = path.join(file_escape(item.name()));
		let ilias = Arc::clone(&ilias);
		spawn(process_gracefully(ilias, path, item));
	}
	Ok(())
}
