use std::{path::Path, sync::Arc};

use anyhow::{Context, Result};
use once_cell::sync::Lazy;
use scraper::{Html, Selector};

use crate::{ilias::Object, process_gracefully, queue::spawn, util::file_escape};

use super::{ILIAS, URL};

static LINKS: Lazy<Selector> = Lazy::new(|| Selector::parse("a").unwrap());
static TABLE_HEADER: Lazy<Selector> = Lazy::new(|| Selector::parse("th").unwrap());
static TABLE_ROW: Lazy<Selector> = Lazy::new(|| Selector::parse("tr").unwrap());
static TABLE_CELLS: Lazy<Selector> = Lazy::new(|| Selector::parse("td").unwrap());

static FORUM_PAGES: Lazy<Selector> =
	Lazy::new(|| Selector::parse("div.ilTableNav > table > tbody > tr > td > a").unwrap());

const NO_ENTRIES: &str = "Keine Einträge";

pub async fn download(path: &Path, ilias: Arc<ILIAS>, url: &URL) -> Result<()> {
	if !ilias.opt.forum {
		return Ok(());
	}
	let url = &url.url;
	let html = {
		let data = ilias.download(url);
		let html_text = data.await?.text().await?;
		let url = {
			let html = Html::parse_document(&html_text);
			let thread_count_selector = html
				.select(&LINKS)
				.flat_map(|x| x.value().attr("href"))
				.find(|x| x.contains("trows=800"));
			if thread_count_selector.is_none() {
				if let Some(cell) = html.select(&TABLE_CELLS).next() {
					if cell.text().any(|x| x == NO_ENTRIES) {
						return Ok(()); // empty forum
					}
				}
			}
			thread_count_selector
				.context("can't find forum thread count selector (empty forum?)")?
				.to_owned()
		};
		let data = ilias.download(&url);
		let html = data.await?.text().await?;
		Html::parse_document(&html)
	};
	for row in html.select(&TABLE_ROW) {
		if row.value().attr("class") == Some("hidden-print") {
			continue; // thread count
		}
		if row.select(&TABLE_HEADER).next().is_some() {
			continue;
		}
		let cells = row.select(&TABLE_CELLS).collect::<Vec<_>>();
		if cells.len() != 6 {
			warning!(format =>
				"Warning: {}{} {} {}",
				"unusual table row (", cells.len(), "cells) in", url.to_string()
			);
			continue;
		}
		let link = cells[1].select(&LINKS).next().context("thread link not found")?;
		let object = Object::from_link(link, link)?;
		let mut path = path.to_owned();
		let name = format!(
			"{}_{}",
			object.url().thr_pk.as_ref().context("thr_pk not found for thread")?,
			link.text().collect::<String>().trim()
		);
		path.push(file_escape(&name));
		// FIXME: this heuristic no longer works after downloading attachments
		// TODO: set modification date?
		let saved_posts = {
			match std::fs::read_dir(&path) {
				// TODO: make this async
				Ok(stream) => stream.count(),
				Err(_) => 0,
			}
		};
		let available_posts = cells[3]
			.text()
			.next()
			.unwrap_or_default()
			.trim()
			.parse::<usize>()
			.context("parsing post count failed")?;
		if available_posts <= saved_posts && !ilias.opt.force {
			continue;
		}
		let ilias = Arc::clone(&ilias);
		spawn(process_gracefully(ilias, path, object));
	}
	if html.select(&FORUM_PAGES).count() > 0 {
		log!(0, "Ignoring older threads in {:?}..", path);
	}
	Ok(())
}
