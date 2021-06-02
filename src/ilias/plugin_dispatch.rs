use std::{path::Path, sync::Arc};

use anyhow::{Context, Result};
use once_cell::sync::Lazy;
use reqwest::Url;
use scraper::{Html, Selector};

use crate::{ilias::Object, process_gracefully, queue::spawn, util::file_escape, ILIAS_URL};

use super::{ILIAS, URL};

static LINKS: Lazy<Selector> = Lazy::new(|| Selector::parse("a").unwrap());
static A_TARGET_BLANK: Lazy<Selector> = Lazy::new(|| Selector::parse(r#"a[target="_blank"]"#).unwrap());
static VIDEO_ROWS: Lazy<Selector> = Lazy::new(|| Selector::parse(".ilTableOuter > div > table > tbody > tr").unwrap());
static TABLE_CELLS: Lazy<Selector> = Lazy::new(|| Selector::parse("td").unwrap());

const NO_ENTRIES: &str = "Keine Einträge";

pub async fn download(path: &Path, ilias: Arc<ILIAS>, url: &URL) -> Result<()> {
	if ilias.opt.no_videos {
		return Ok(());
	}
	let full_url = {
		// first find the link to full video list
		let list_url = format!("{}ilias.php?ref_id={}&cmdClass=xocteventgui&cmdNode=nc:n4:14u&baseClass=ilObjPluginDispatchGUI&lang=de&limit=20&cmd=asyncGetTableGUI&cmdMode=asynch", ILIAS_URL, url.ref_id);
		log!(1, "Loading {}", list_url);
		let data = ilias.download(&list_url).await?;
		let html = data.text().await?;
		let html = Html::parse_fragment(&html);
		html.select(&LINKS)
			.filter_map(|link| link.value().attr("href"))
			.filter(|href| href.contains("trows=800"))
			.map(|x| x.to_string())
			.next()
			.context("video list link not found")?
	};
	log!(1, "Rewriting {}", full_url);
	let mut full_url = Url::parse(&format!("{}{}", ILIAS_URL, full_url))?;
	let mut query_parameters = full_url
		.query_pairs()
		.map(|(x, y)| (x.into_owned(), y.into_owned()))
		.collect::<Vec<_>>();
	for (key, value) in &mut query_parameters {
		match key.as_ref() {
			"cmd" => *value = "asyncGetTableGUI".into(),
			"cmdClass" => *value = "xocteventgui".into(),
			_ => {},
		}
	}
	query_parameters.push(("cmdMode".into(), "asynch".into()));
	full_url
		.query_pairs_mut()
		.clear()
		.extend_pairs(&query_parameters)
		.finish();
	log!(1, "Loading {}", full_url);
	let data = ilias.download(full_url.as_str()).await?;
	let html = data.text().await?;
	let html = Html::parse_fragment(&html);
	for row in html.select(&VIDEO_ROWS) {
		let link = row.select(&A_TARGET_BLANK).next();
		if link.is_none() {
			if !row.text().any(|x| x == NO_ENTRIES) {
				warning!(format => "table row without link in {}", url.url);
			}
			continue;
		}
		let link = link.unwrap();
		let mut cells = row.select(&TABLE_CELLS);
		if let Some(title) = cells.nth(2) {
			let title = title.text().collect::<String>();
			let title = title.trim();
			if title.starts_with("<div") {
				continue;
			}
			let mut path = path.to_owned();
			path.push(format!("{}.mp4", file_escape(title)));
			log!(1, "Found video: {}", title);
			let video = Object::Video {
				url: URL::raw(link.value().attr("href").context("video link without href")?.to_owned()),
			};
			let ilias = Arc::clone(&ilias);
			spawn(process_gracefully(ilias, path, video));
		}
	}
	Ok(())
}
