use std::{path::Path, sync::Arc};

use anyhow::{anyhow, Context, Result};
use once_cell::sync::Lazy;
use regex::Regex;
use scraper::Selector;

use crate::{
	handle_gracefully, process_gracefully,
	queue::spawn,
	util::{file_escape, wrap_html, write_file_data},
};

use super::{Object, ILIAS, URL};

static LINKS: Lazy<Selector> = Lazy::new(|| Selector::parse("a").unwrap());
static IMAGES: Lazy<Selector> = Lazy::new(|| Selector::parse("img").unwrap());
static TABLES: Lazy<Selector> = Lazy::new(|| Selector::parse("table").unwrap());
static LINK_IN_TABLE: Lazy<Selector> = Lazy::new(|| Selector::parse("tbody tr td a").unwrap());
static POST_ROW: Lazy<Selector> = Lazy::new(|| Selector::parse(".ilFrmPostRow").unwrap());
static POST_TITLE: Lazy<Selector> = Lazy::new(|| Selector::parse(".ilFrmPostTitle").unwrap());
static POST_CONTAINER: Lazy<Selector> = Lazy::new(|| Selector::parse(".ilFrmPostContentContainer").unwrap());
static POST_ATTACHMENTS: Lazy<Selector> = Lazy::new(|| Selector::parse(".ilFrmPostAttachmentsContainer").unwrap());
static SPAN_SMALL: Lazy<Selector> = Lazy::new(|| Selector::parse("span.small").unwrap());
static IMAGE_SRC_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r#"\./data/produktiv/mobs/mm_(\d+)/([^?]+).+"#).unwrap());

pub async fn download(path: &Path, relative_path: &Path, ilias: Arc<ILIAS>, url: &URL) -> Result<()> {
	if !ilias.opt.forum {
		return Ok(());
	}
	let mut all_images = Vec::new();
	let mut attachments = Vec::new();
	{
		let html = ilias.get_html(&url.url).await?;
		for post in html.select(&POST_ROW) {
			let title = post
				.select(&POST_TITLE)
				.next()
				.context("post title not found")?
				.text()
				.collect::<String>();
			let author = post.select(&SPAN_SMALL).next().context("post author not found")?;
			let author = author.text().collect::<String>();
			let author = author.trim().split('|').collect::<Vec<_>>();
			let author = if author.len() == 2 {
				author[0] // pseudonymous forum
			} else if author.len() == 3 {
				if author[1] != "Pseudonym" {
					author[1]
				} else {
					author[0]
				}
			} else {
				return Err(anyhow!("author data in unknown format"));
			}
			.trim();
			let container = post
				.select(&POST_CONTAINER)
				.next()
				.context("post container not found")?;
			let link = container.select(&LINKS).next().context("post link not found")?;
			let id = link.value().attr("id").context("no id in thread link")?.to_owned();
			let name = format!("{}_{}_{}.html", id, author, title.trim());
			let data = wrap_html(&container.inner_html());
			let path = path.join(file_escape(&name));
			let relative_path = relative_path.join(file_escape(&name));
			spawn(handle_gracefully(async move {
				log!(0, "Writing {}", relative_path.display());
				write_file_data(&path, &mut data.as_bytes())
					.await
					.context("failed to write forum post")
			}));
			let images = container
				.select(&IMAGES)
				.map(|x| x.value().attr("src").map(|x| x.to_owned()));
			for image in images {
				let image = image.context("no src on image")?;
				all_images.push((id.clone(), image));
			}
			if let Some(container) = container.select(&POST_ATTACHMENTS).next() {
				for attachment in container.select(&LINKS) {
					let href = attachment
						.value()
						.attr("href")
						.map(|x| x.to_owned())
						.context("attachment link without href")?;
					if href.contains("cmd=deliverZipFile") {
						continue; // skip downloading all attachments as zip
					}
					attachments.push((id.clone(), attachment.text().collect::<String>(), href));
				}
			}
		}
		// pagination
		if let Some(pages) = html.select(&TABLES).next() {
			if let Some(last) = pages.select(&LINK_IN_TABLE).last() {
				let text = last.text().collect::<String>();
				if text.trim() == ">>" {
					// not last page yet
					let ilias = Arc::clone(&ilias);
					let next_page = Object::Thread {
						url: URL::from_href(last.value().attr("href").context("page link not found")?)?,
					};
					spawn(process_gracefully(ilias, path.to_owned(), next_page));
				}
			} else {
				log!(
					0,
					"Warning: {} {}",
					"unable to find pagination links in".bright_yellow(),
					url.url.to_string().bright_yellow()
				);
			}
		}
	}
	for (id, image) in all_images {
		let src = URL::from_href(&image)?;
		let dl = ilias.download(&src.url).await?;
		let mut path = path.to_owned();
		let file_name = if let Some(m) = IMAGE_SRC_REGEX.captures(&image) {
			// image uploaded to ILIAS
			let (media_id, filename) = (m.get(1).unwrap().as_str(), m.get(2).unwrap().as_str());
			file_escape(&format!("{}_{}_{}", id, media_id, filename))
		} else {
			// external image
			file_escape(&format!("{}_{}", id, image))
		};
		path.push(&file_name);
		let relative_path = relative_path.join(file_name);
		spawn(handle_gracefully(async move {
			let bytes = dl.bytes().await?;
			log!(0, "Writing {}", relative_path.display());
			write_file_data(&path, &mut &*bytes)
				.await
				.context("failed to write forum post image attachment")
		}));
	}
	for (id, name, url) in attachments {
		let src = URL::from_href(&url)?;
		let dl = ilias.download(&src.url).await?;
		let mut path = path.to_owned();
		let file_name = file_escape(&format!("{}_{}", id, name));
		path.push(&file_name);
		let relative_path = relative_path.join(file_name);
		spawn(handle_gracefully(async move {
			let bytes = dl.bytes().await?;
			log!(0, "Writing {}", relative_path.display());
			write_file_data(&path, &mut &*bytes)
				.await
				.context("failed to write forum post file attachment")
		}));
	}
	Ok(())
}
