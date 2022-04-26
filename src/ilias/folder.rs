use std::{path::Path, sync::Arc, collections::HashSet};

use anyhow::{Context, Result};

use crate::{
	process_gracefully,
	queue::spawn,
	util::{file_escape, write_file_data},
};

use super::{ILIAS, URL};

pub async fn download(path: &Path, ilias: Arc<ILIAS>, url: &URL) -> Result<()> {
	let content = ilias.get_course_content(&url).await?;
	if ilias.opt.save_ilias_pages {
		if let Some(s) = content.1.as_ref() {
			let path = path.join("folder.html");
			write_file_data(&path, &mut s.as_bytes())
				.await
				.context("failed to write folder page html")?;
		}
	}
	let mut names = HashSet::new();
	for item in content.0 {
		let item = item?;
		let item_name = file_escape(
			ilias.course_names.get(item.name()).map(|x| &**x).unwrap_or(item.name()),
		);
		if names.contains(&item_name) {
			warning!(format => "folder {} contains duplicated folder {:?}", path.display(), item_name);
		}
		names.insert(item_name.clone());
		let path = path.join(item_name);
		let ilias = Arc::clone(&ilias);
		spawn(process_gracefully(ilias, path, item));
	}
	Ok(())
}
