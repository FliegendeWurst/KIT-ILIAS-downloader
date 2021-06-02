use std::{path::Path, sync::Arc};

use anyhow::{Context, Result};
use once_cell::sync::Lazy;
use regex::Regex;
use tokio::fs;

use crate::{util::write_stream_to_file, ILIAS_URL};

use super::{ILIAS, URL};

static XOCT_REGEX: Lazy<Regex> =
	Lazy::new(|| Regex::new(r#"(?m)<script>\s+xoctPaellaPlayer\.init\(([\s\S]+)\)\s+</script>"#).unwrap());

pub async fn download(path: &Path, relative_path: &Path, ilias: Arc<ILIAS>, url: &URL) -> Result<()> {
	if ilias.opt.no_videos {
		return Ok(());
	}
	if fs::metadata(&path).await.is_ok() && !(ilias.opt.force || ilias.opt.check_videos) {
		log!(2, "Skipping download, file exists already");
		return Ok(());
	}
	let url = format!("{}{}", ILIAS_URL, url.url);
	let data = ilias.download(&url);
	let html = data.await?.text().await?;
	log!(2, "{}", html);
	let json: serde_json::Value = {
		let mut json_capture = XOCT_REGEX.captures_iter(&html);
		let json = &json_capture.next().context("xoct player json not found")?[1];
		log!(2, "{}", json);
		let json = json.split(",\n").next().context("invalid xoct player json")?;
		serde_json::from_str(&json.trim())?
	};
	log!(2, "{}", json);
	let url = json
		.pointer("/streams/0/sources/mp4/0/src")
		.context("video src not found")?
		.as_str()
		.context("video src not string")?;
	let meta = fs::metadata(&path).await;
	if !ilias.opt.force && meta.is_ok() && ilias.opt.check_videos {
		let head = ilias.head(url).await.context("HEAD request failed")?;
		if let Some(len) = head.headers().get("content-length") {
			if meta?.len() != len.to_str()?.parse::<u64>()? {
				warning!(
					relative_path.to_string_lossy(),
					"was updated, consider moving the outdated file"
				);
			}
		}
	} else {
		let resp = ilias.download(&url).await?;
		log!(0, "Writing {}", relative_path.to_string_lossy());
		write_stream_to_file(&path, resp.bytes_stream()).await?;
	}
	Ok(())
}
