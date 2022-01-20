use std::{
	path::{Path, PathBuf},
	process::Stdio,
	sync::Arc,
};

use anyhow::{Context, Result};
use once_cell::sync::Lazy;
use regex::Regex;
use tempfile::tempdir;
use tokio::{fs, process::Command};

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
	let streams = json
		.get("streams")
		.context("video streams not found")?
		.as_array()
		.context("video streams not an array")?;
	if streams.len() == 1 {
		let url = streams[0]
			.pointer("/sources/mp4/0/src")
			.context("video src not found")?
			.as_str()
			.context("video src not string")?;
		download_to_path(&ilias, path, relative_path, url).await?;
	} else {
		if !ilias.opt.combine_videos {
			fs::create_dir(path).await.context("failed to create video directory")?;
			download_all(path, streams, ilias, relative_path).await?;
		} else {
			let dir = tempdir()?;
			// construct ffmpeg command to combine all files
			let mut arguments = vec![];
			for file in download_all(dir.path(), streams, ilias, relative_path).await? {
				arguments.push("-i".to_owned());
				arguments.push(file.to_str().context("invalid UTF8")?.into());
			}
			arguments.push("-c".into());
			arguments.push("copy".into());
			for i in 0..(arguments.len() / 2) - 1 {
				arguments.push("-map".into());
				arguments.push(format!("{}", i));
			}
			arguments.push(path.to_str().context("invalid UTF8 in path")?.into());
			let status = Command::new("ffmpeg")
				.args(&arguments)
				.stderr(Stdio::null())
				.stdout(Stdio::null())
				.spawn()
				.context("failed to start ffmpeg")?
				.wait()
				.await
				.context("failed to wait for ffmpeg")?;
			if !status.success() {
				error!(format!("ffmpeg failed to merge video files into {}", path.display()));
				error!(format!("check this directory: {}", dir.into_path().display()));
				error!(format!("ffmpeg command: {}", arguments.join(" ")));
			}
		};
	}
	Ok(())
}

async fn download_all(
	path: &Path,
	streams: &Vec<serde_json::Value>,
	ilias: Arc<ILIAS>,
	relative_path: &Path,
) -> Result<Vec<PathBuf>> {
	let mut paths = Vec::new();
	for (i, stream) in streams.into_iter().enumerate() {
		let url = stream
			.pointer("/sources/mp4/0/src")
			.context("video src not found")?
			.as_str()
			.context("video src not string")?;
		let new_path = path.join(format!("Stream{}.mp4", i + 1));
		download_to_path(
			&ilias,
			&new_path,
			&relative_path.join(format!("Stream{}.mp4", i + 1)),
			url,
		)
		.await?;
		paths.push(new_path);
	}
	Ok(paths)
}

async fn download_to_path(ilias: &ILIAS, path: &Path, relative_path: &Path, url: &str) -> Result<()> {
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
