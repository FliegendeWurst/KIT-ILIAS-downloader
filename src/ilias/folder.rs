use std::{collections::HashSet, path::Path, sync::Arc};

use anyhow::{Context, Result};
use async_recursion::async_recursion;
use once_cell::sync::Lazy;
use regex::Regex;

use crate::{
	process_gracefully,
	queue::spawn,
	util::{file_escape, write_file_data},
};

use super::{ILIAS, URL};

static EXPAND_LINK: Lazy<Regex> = Lazy::new(|| Regex::new("expand=\\d").unwrap());

#[async_recursion]
pub async fn download(path: &Path, ilias: Arc<ILIAS>, url: &URL) -> Result<()> {
	let content = ilias.get_course_content(url).await?;

	// expand all sessions
	for href in content.2 {
		// link format: ilias.php?ref_id=1943526&expand=2602906&cmd=view&cmdClass=ilobjfoldergui&cmdNode=x1:nk&baseClass=ilrepositorygui#lg_div_1948579_pref_1943526
		if EXPAND_LINK.is_match(&href) {
			return download(path, ilias, &URL::from_href(&href)?).await;
		}
	}

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
		let item_name = file_escape(ilias.course_names.get(item.name()).map(|x| &**x).unwrap_or(item.name()));
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
