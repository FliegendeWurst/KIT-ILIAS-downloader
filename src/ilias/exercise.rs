use std::{collections::HashSet, path::Path, sync::Arc};

use anyhow::{Context, Result};
use once_cell::sync::Lazy;
use scraper::Selector;

use crate::{process_gracefully, queue::spawn, util::file_escape};

use super::{Object, ILIAS, URL};

static LINKS: Lazy<Selector> = Lazy::new(|| Selector::parse("a").unwrap());
static FORM_GROUP: Lazy<Selector> = Lazy::new(|| Selector::parse(".form-group").unwrap());
static FORM_NAME: Lazy<Selector> = Lazy::new(|| Selector::parse(".il_InfoScreenProperty").unwrap());

pub async fn download(path: &Path, ilias: Arc<ILIAS>, url: &URL) -> Result<()> {
	let html = ilias.get_html(&url.url).await?;
	let mut filenames = HashSet::new();
	for row in html.select(&FORM_GROUP) {
		let link = row.select(&LINKS).next();
		if link.is_none() {
			continue;
		}
		let link = link.unwrap();
		let href = link.value().attr("href");
		if href.is_none() {
			continue;
		}
		let href = href.unwrap();
		let url = URL::from_href(href)?;
		let cmd = url.cmd.as_deref().unwrap_or("");
		if cmd != "downloadFile" && cmd != "downloadGlobalFeedbackFile" && cmd != "downloadFeedbackFile" {
			continue;
		}
		// link is definitely just a download link to the exercise or the solution
		let name = row
			.select(&FORM_NAME)
			.next()
			.context("link without file name")?
			.text()
			.collect::<String>()
			.trim()
			.to_owned();
		let item = Object::File { url, name };
		let mut path = path.to_owned();
		// handle files with the same name
		let filename = file_escape(item.name());
		let mut parts = filename.rsplitn(2, '.');
		let extension = parts.next().unwrap_or(&filename);
		let name = parts.next().unwrap_or("");
		let mut unique_filename = filename.clone();
		let mut i = 1;
		while filenames.contains(&unique_filename) {
			i += 1;
			if name.is_empty() {
				unique_filename = format!("{}{}", extension, i);
			} else {
				unique_filename = format!("{}{}.{}", name, i, extension);
			}
		}
		filenames.insert(unique_filename.clone());
		path.push(unique_filename);
		let ilias = Arc::clone(&ilias);
		spawn(process_gracefully(ilias, path, item));
	}
	Ok(())
}
