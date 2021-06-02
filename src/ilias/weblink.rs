use std::{path::Path, sync::Arc};

use anyhow::{Context, Result};
use once_cell::sync::Lazy;
use scraper::Selector;
use tokio::fs;

use crate::{
	util::{create_dir, file_escape, write_file_data},
	ILIAS_URL,
};

use super::{ILIAS, URL};

static LINKS: Lazy<Selector> = Lazy::new(|| Selector::parse("a").unwrap());

pub async fn download(path: &Path, relative_path: &Path, ilias: Arc<ILIAS>, url: &URL) -> Result<()> {
	if !ilias.opt.force && fs::metadata(&path).await.is_ok() {
		log!(2, "Skipping download, link exists already");
		return Ok(());
	}
	let head_req_result = ilias.head(&url.url).await;
	let url = match &head_req_result {
		Err(e) => e.url().context("HEAD request failed")?.as_str(),
		Ok(head) => head.url().as_str(),
	};
	if url.starts_with(ILIAS_URL) {
		// is a link list
		if fs::metadata(&path).await.is_err() {
			create_dir(&path).await?;
			log!(0, "Writing {}", relative_path.to_string_lossy());
		}

		let urls = {
			let html = ilias.get_html(url).await?;
			html.select(&LINKS)
				.filter_map(|x| x.value().attr("href").map(|y| (y, x.text().collect::<String>())))
				.map(|(x, y)| {
					URL::from_href(x)
						.map(|z| (z, y.trim().to_owned()))
						.context("parsing weblink")
				})
				.collect::<Result<Vec<_>>>()
		}?;

		for (url, name) in urls {
			if url.cmd.as_deref().unwrap_or("") != "callLink" {
				continue;
			}

			let head = ilias
				.head(url.url.as_str())
				.await
				.context("HEAD request to web link failed");
			if let Some(err) = head.as_ref().err() {
				warning!(err);
				continue;
			}
			let head = head.unwrap();
			let url = head.url().as_str();
			write_file_data(path.join(file_escape(&name)), &mut url.as_bytes()).await?;
		}
	} else {
		log!(0, "Writing {}", relative_path.to_string_lossy());
		write_file_data(&path, &mut url.as_bytes())
			.await
			.context("failed to save weblink URL")?;
	}
	Ok(())
}
