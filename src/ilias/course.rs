use std::{path::PathBuf, sync::Arc};

use anyhow::{Context, Result};
use once_cell::sync::Lazy;
use regex::Regex;

use crate::{
	process_gracefully,
	queue::spawn,
	util::{file_escape, write_file_data},
};

use super::{ILIAS, URL};

static CMD_NODE_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r#"cmdNode=uf:\w\w"#).unwrap());

pub async fn download(path: PathBuf, ilias: Arc<ILIAS>, url: &URL, name: &str) -> Result<()> {
	let content = if ilias.opt.content_tree {
		let html = ilias.download(&url.url).await?.text().await?;
		let cmd_node = CMD_NODE_REGEX.find(&html).context("can't find cmdNode")?.as_str()[8..].to_owned();
		let content_tree = ilias.get_course_content_tree(&url.ref_id, &cmd_node).await;
		match content_tree {
			Ok(tree) => (tree.into_iter().map(Result::Ok).collect(), None),
			Err(e) => {
				// some folders are hidden on the course page and can only be found via the RSS feed / recent activity / content tree sidebar
				// TODO: this is probably never the case for folders?
				if html.contains(r#"input[name="cmd[join]""#) {
					return Ok(()); // ignore groups we are not in
				}
				warning!(name, "falling back to incomplete course content extractor!", e);
				ilias.get_course_content(&url).await? // TODO: perhaps don't download almost the same content 3x
			},
		}
	} else {
		ilias.get_course_content(&url).await?
	};
	if let Some(s) = content.1.as_ref() {
		let path = path.join("course.html");
		write_file_data(&path, &mut s.as_bytes())
			.await
			.context("failed to write course page html")?;
	}
	for item in content.0 {
		let item = item?;
		let path = path.join(file_escape(item.name()));
		let ilias = Arc::clone(&ilias);
		spawn(process_gracefully(ilias, path, item));
	}
	Ok(())
}
