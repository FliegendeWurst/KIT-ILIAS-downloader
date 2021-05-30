// SPDX-License-Identifier: GPL-3.0-or-later

#![allow(clippy::upper_case_acronyms)]

use anyhow::{anyhow, Context, Result};
use colored::Colorize;
use futures::future::{self, Either};
use futures_channel::mpsc::UnboundedSender;
use futures_util::stream::TryStreamExt;
use futures_util::StreamExt;
use ignore::gitignore::Gitignore;
use indicatif::{ProgressDrawTarget, ProgressStyle};
use once_cell::sync::{Lazy, OnceCell};
use scraper::Html;
use structopt::StructOpt;
use tokio::task::{self, JoinHandle};
use tokio::{fs, sync::Semaphore, time};
use tokio_util::io::StreamReader;
use url::Url;

use std::collections::HashSet;
use std::future::Future;
use std::io;
use std::io::BufReader;
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::SystemTime;

pub const ILIAS_URL: &str = "https://ilias.studium.kit.edu/";

#[macro_use]
mod cli;
use cli::*;
mod ilias;
use ilias::*;
use Object::*;
mod util;
use util::*;

/// Global job queue
static TASKS: OnceCell<UnboundedSender<JoinHandle<()>>> = OnceCell::new();
static TASKS_RUNNING: Lazy<Semaphore> = Lazy::new(|| Semaphore::new(0));
static REQUEST_TICKETS: Lazy<Semaphore> = Lazy::new(|| Semaphore::new(0));

pub async fn get_request_ticket() {
	REQUEST_TICKETS.acquire().await.unwrap().forget();
}

macro_rules! spawn {
	($e:expr) => {
		TASKS.get().unwrap().unbounded_send(task::spawn($e)).unwrap();
	};
}

#[tokio::main]
async fn main() {
	let opt = Opt::from_args();
	let rate = opt.rate;
	task::spawn(async move {
		let mut interval = time::interval(time::Duration::from_secs_f64(60.0 / rate as f64));
		loop {
			interval.tick().await;
			REQUEST_TICKETS.add_permits(1);
		}
	});
	if let Err(e) = real_main(opt).await {
		error!(e);
	}
}

async fn try_to_load_session(opt: Opt, ignore: Gitignore) -> Result<ILIAS> {
	let session_path = opt.output.join(".iliassession");
	let meta = tokio::fs::metadata(&session_path).await?;
	let modified = meta.modified()?;
	let now = SystemTime::now();
	// the previous session is only useful if it isn't older than ~1 hour
	let duration = now.duration_since(modified)?;
	if duration.as_secs() <= 60 * 60 {
		let file = std::fs::File::open(session_path)?;
		let cookies = cookie_store::CookieStore::load_json(BufReader::new(file))
			.map_err(|err| anyhow!(err))
			.context("failed to load session cookies")?;
		let cookie_store = reqwest_cookie_store::CookieStoreMutex::new(cookies);
		let cookie_store = std::sync::Arc::new(cookie_store);
		Ok(ILIAS::with_session(opt, cookie_store, ignore).await?)
	} else {
		Err(anyhow!("session data too old"))
	}
}

async fn login(opt: Opt, ignore: Gitignore) -> Result<ILIAS> {
	// load .iliassession file
	if opt.keep_session {
		match try_to_load_session(opt.clone(), ignore.clone())
			.await
			.context("failed to load previous session")
		{
			Ok(ilias) => return Ok(ilias),
			Err(e) => warning!(e),
		}
	}

	// loac .iliaslogin file
	let iliaslogin = opt.output.join(".iliaslogin");
	let login = std::fs::read_to_string(&iliaslogin);
	let (user, pass) = if let Ok(login) = login {
		let mut lines = login.split('\n');
		let user = lines.next().context("missing user in .iliaslogin")?;
		let pass = lines.next().context("missing password in .iliaslogin")?;
		let user = user.trim();
		let pass = pass.trim();
		(user.to_owned(), pass.to_owned())
	} else {
		ask_user_pass(&opt).context("credentials input failed")?
	};

	let ilias = match ILIAS::login(opt, &user, &pass, ignore).await {
		Ok(ilias) => ilias,
		Err(e) => {
			error!(e);
			std::process::exit(77);
		},
	};
	Ok(ilias)
}

async fn real_main(mut opt: Opt) -> Result<()> {
	LOG_LEVEL.store(opt.verbose, Ordering::SeqCst);
	#[cfg(windows)]
	let _ = colored::control::set_virtual_terminal(true);

	create_dir(&opt.output).await.context("failed to create output directory")?;
	// use UNC paths on Windows (to avoid the default max. path length of 255)
	opt.output = fs::canonicalize(opt.output).await.context("failed to canonicalize output directory")?;

	// load .iliasignore file
	let (ignore, error) = Gitignore::new(opt.output.join(".iliasignore"));
	if let Some(err) = error {
		warning!(err);
	}

	let ilias = login(opt, ignore).await?;

	if ilias.opt.content_tree {
		if let Err(e) = ilias
			.download("ilias.php?baseClass=ilRepositoryGUI&cmd=frameset&set_mode=tree&ref_id=1")
			.await
		{
			warning!("could not enable content tree:", e);
		}
	}
	let ilias = Arc::new(ilias);
	let (tx, mut rx) = futures_channel::mpsc::unbounded::<JoinHandle<()>>();
	TASKS.get_or_init(|| tx.clone());
	TASKS_RUNNING.add_permits(ilias.opt.jobs);
	PROGRESS_BAR_ENABLED.store(atty::is(atty::Stream::Stdout), Ordering::SeqCst);
	if PROGRESS_BAR_ENABLED.load(Ordering::SeqCst) {
		PROGRESS_BAR.set_draw_target(ProgressDrawTarget::stderr_nohz());
		PROGRESS_BAR.set_style(ProgressStyle::default_bar().template("[{pos}/{len}+] {wide_msg}"));
		PROGRESS_BAR.set_message("initializing..");
	}

	let sync_url = ilias.opt.sync_url.clone().unwrap_or_else(|| {
		// default sync URL: main personal desktop
		format!("{}ilias.php?baseClass=ilPersonalDesktopGUI&cmd=jumpToSelectedItems", ILIAS_URL)
	});
	let obj = Object::from_url(URL::from_href(&sync_url).context("invalid sync URL")?, String::new(), None).context("invalid sync object")?; // name can be empty for first element
	spawn!(process_gracefully(ilias.clone(), ilias.opt.output.clone(), obj));

	while let Either::Left((task, _)) = future::select(rx.next(), future::ready(())).await {
		if let Some(task) = task {
			if let Err(e) = task.await {
				error!(e)
			}
		} else {
			break; // channel is empty => all tasks are completed
		}
	}
	if ilias.opt.content_tree {
		if let Err(e) = ilias
			.download("ilias.php?baseClass=ilRepositoryGUI&cmd=frameset&set_mode=flat&ref_id=1")
			.await
		{
			warning!("could not disable content tree:", e);
		}
	}
	if ilias.opt.keep_session {
		if let Err(e) = ilias.save_session().await.context("failed to save session cookies") {
			warning!(e)
		}
	}
	if PROGRESS_BAR_ENABLED.load(Ordering::SeqCst) {
		PROGRESS_BAR.set_style(ProgressStyle::default_bar().template("[{pos}/{len}] {wide_msg}"));
		PROGRESS_BAR.finish_with_message("done");
	}
	Ok(())
}

// https://github.com/rust-lang/rust/issues/53690#issuecomment-418911229
#[allow(clippy::manual_async_fn)]
fn process_gracefully(ilias: Arc<ILIAS>, path: PathBuf, obj: Object) -> impl Future<Output = ()> + Send {
	async move {
		if PROGRESS_BAR_ENABLED.load(Ordering::SeqCst) {
			PROGRESS_BAR.inc_length(1);
		}
		let permit = TASKS_RUNNING.acquire().await.unwrap();
		let path_text = path.to_string_lossy().into_owned();
		if let Err(e) = process(ilias, path, obj).await.context("failed to process URL") {
			error!("Syncing {}", path_text; e);
		}
		drop(permit);
	}
}

async fn handle_gracefully(fut: impl Future<Output = Result<()>>) {
	if let Err(e) = fut.await {
		error!(e);
	}
}

#[allow(non_upper_case_globals)]
mod selectors {
	use once_cell::sync::Lazy;
	use regex::Regex;
	use scraper::Selector;
	// construct CSS selectors once
	pub static LINKS: Lazy<Selector> = Lazy::new(|| Selector::parse("a").unwrap());
	pub static a_target_blank: Lazy<Selector> = Lazy::new(|| Selector::parse(r#"a[target="_blank"]"#).unwrap());
	pub static IMAGES: Lazy<Selector> = Lazy::new(|| Selector::parse("img").unwrap());
	pub static TABLES: Lazy<Selector> = Lazy::new(|| Selector::parse("table").unwrap());
	pub static VIDEO_ROWS: Lazy<Selector> = Lazy::new(|| Selector::parse(".ilTableOuter > div > table > tbody > tr").unwrap());
	pub static links_in_table: Lazy<Selector> = Lazy::new(|| Selector::parse("tbody tr td a").unwrap());
	pub static th: Lazy<Selector> = Lazy::new(|| Selector::parse("th").unwrap());
	pub static td: Lazy<Selector> = Lazy::new(|| Selector::parse("td").unwrap());
	pub static tr: Lazy<Selector> = Lazy::new(|| Selector::parse("tr").unwrap());
	pub static post_row: Lazy<Selector> = Lazy::new(|| Selector::parse(".ilFrmPostRow").unwrap());
	pub static post_title: Lazy<Selector> = Lazy::new(|| Selector::parse(".ilFrmPostTitle").unwrap());
	pub static post_container: Lazy<Selector> = Lazy::new(|| Selector::parse(".ilFrmPostContentContainer").unwrap());
	pub static post_attachments: Lazy<Selector> = Lazy::new(|| Selector::parse(".ilFrmPostAttachmentsContainer").unwrap());
	pub static span_small: Lazy<Selector> = Lazy::new(|| Selector::parse("span.small").unwrap());
	pub static forum_pages: Lazy<Selector> = Lazy::new(|| Selector::parse("div.ilTableNav > table > tbody > tr > td > a").unwrap());
	pub static alert_danger: Lazy<Selector> = Lazy::new(|| Selector::parse("div.alert-danger").unwrap());
	pub static form_group: Lazy<Selector> = Lazy::new(|| Selector::parse(".form-group").unwrap());
	pub static form_name: Lazy<Selector> = Lazy::new(|| Selector::parse(".il_InfoScreenProperty").unwrap());
	pub static cmd_node_regex: Lazy<Regex> = Lazy::new(|| Regex::new(r#"cmdNode=uf:\w\w"#).unwrap());
	pub static image_src_regex: Lazy<Regex> = Lazy::new(|| Regex::new(r#"\./data/produktiv/mobs/mm_(\d+)/([^?]+).+"#).unwrap());
	pub static XOCT_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r#"(?m)<script>\s+xoctPaellaPlayer\.init\(([\s\S]+)\)\s+</script>"#).unwrap());
	pub static il_content_container: Lazy<Selector> = Lazy::new(|| Selector::parse("#il_center_col").unwrap());
	pub static item_prop: Lazy<Selector> = Lazy::new(|| Selector::parse("span.il_ItemProperty").unwrap());
	pub static container_items: Lazy<Selector> = Lazy::new(|| Selector::parse("div.il_ContainerListItem").unwrap());
	pub static container_item_title: Lazy<Selector> = Lazy::new(|| Selector::parse("a.il_ContainerItemTitle").unwrap());
}
use crate::selectors::*;

const NO_ENTRIES: &str = "Keine Einträge";

async fn process(ilias: Arc<ILIAS>, path: PathBuf, obj: Object) -> Result<()> {
	let relative_path = path.strip_prefix(&ilias.opt.output).unwrap();
	if PROGRESS_BAR_ENABLED.load(Ordering::SeqCst) {
		PROGRESS_BAR.inc(1);
		let path = relative_path.display().to_string();
		if !path.is_empty() {
			PROGRESS_BAR.set_message(path);
		}
	}
	// root path should not be matched
	if relative_path.parent().is_some() && ilias.ignore.matched(relative_path, obj.is_dir()).is_ignore() {
		log!(1, "Ignored {}", relative_path.to_string_lossy());
		return Ok(());
	}
	log!(1, "Syncing {} {}", obj.kind(), relative_path.to_string_lossy());
	log!(2, " URL: {}", obj.url().url);
	if obj.is_dir() {
		create_dir(&path).await?;
	}
	match &obj {
		Course { url, name } => {
			let content = if ilias.opt.content_tree {
				let html = ilias.download(&url.url).await?.text().await?;
				let cmd_node = cmd_node_regex.find(&html).context("can't find cmdNode")?.as_str()[8..].to_owned();
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
				spawn!(process_gracefully(ilias, path, item));
			}
		},
		Folder { url, .. } | PersonalDesktop { url } => {
			let content = ilias.get_course_content(&url).await?;
			if let Some(s) = content.1.as_ref() {
				let path = path.join("folder.html");
				write_file_data(&path, &mut s.as_bytes())
					.await
					.context("failed to write folder page html")?;
			}
			for item in content.0 {
				let item = item?;
				let path = path.join(file_escape(item.name()));
				let ilias = Arc::clone(&ilias);
				spawn!(process_gracefully(ilias, path, item));
			}
		},
		File { url, .. } => {
			if ilias.opt.skip_files {
				return Ok(());
			}
			if !ilias.opt.force && fs::metadata(&path).await.is_ok() {
				log!(2, "Skipping download, file exists already");
				return Ok(());
			}
			let data = ilias.download(&url.url).await?;
			let mut reader = StreamReader::new(data.bytes_stream().map_err(|x| io::Error::new(io::ErrorKind::Other, x)));
			log!(0, "Writing {}", relative_path.to_string_lossy());
			write_file_data(&path, &mut reader).await?;
		},
		PluginDispatch { url, .. } => {
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
			let mut query_parameters = full_url.query_pairs().map(|(x, y)| (x.into_owned(), y.into_owned())).collect::<Vec<_>>();
			for (key, value) in &mut query_parameters {
				match key.as_ref() {
					"cmd" => *value = "asyncGetTableGUI".into(),
					"cmdClass" => *value = "xocteventgui".into(),
					_ => {},
				}
			}
			query_parameters.push(("cmdMode".into(), "asynch".into()));
			full_url.query_pairs_mut().clear().extend_pairs(&query_parameters).finish();
			log!(1, "Loading {}", full_url);
			let data = ilias.download(full_url.as_str()).await?;
			let html = data.text().await?;
			let html = Html::parse_fragment(&html);
			for row in html.select(&VIDEO_ROWS) {
				let link = row.select(&a_target_blank).next();
				if link.is_none() {
					if !row.text().any(|x| x == NO_ENTRIES) {
						warning!(format => "table row without link in {}", url.url);
					}
					continue;
				}
				let link = link.unwrap();
				let mut cells = row.select(&td);
				if let Some(title) = cells.nth(2) {
					let title = title.text().collect::<String>();
					let title = title.trim();
					if title.starts_with("<div") {
						continue;
					}
					let mut path = path.clone();
					path.push(format!("{}.mp4", file_escape(title)));
					log!(1, "Found video: {}", title);
					let video = Video {
						url: URL::raw(link.value().attr("href").context("video link without href")?.to_owned()),
					};
					let ilias = Arc::clone(&ilias);
					spawn!(process_gracefully(ilias, path, video));
				}
			}
		},
		Video { url } => {
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
						warning!(relative_path.to_string_lossy(), "was updated, consider moving the outdated file");
					}
				}
			} else {
				let resp = ilias.download(&url).await?;
				let mut reader = StreamReader::new(resp.bytes_stream().map_err(|x| io::Error::new(io::ErrorKind::Other, x)));
				log!(0, "Writing {}", relative_path.to_string_lossy());
				write_file_data(&path, &mut reader).await?;
			}
		},
		Forum { url, .. } => {
			if !ilias.opt.forum {
				return Ok(());
			}
			let url = &url.url;
			let html = {
				let data = ilias.download(url);
				let html_text = data.await?.text().await?;
				let url = {
					let html = Html::parse_document(&html_text);
					let thread_count_selector = html.select(&LINKS).flat_map(|x| x.value().attr("href")).find(|x| x.contains("trows=800"));
					if thread_count_selector.is_none() {
						if let Some(cell) = html.select(&td).next() {
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
			for row in html.select(&tr) {
				if row.value().attr("class") == Some("hidden-print") {
					continue; // thread count
				}
				if row.select(&th).next().is_some() {
					continue; // table header
				}
				let cells = row.select(&td).collect::<Vec<_>>();
				if cells.len() != 6 {
					warning!(format =>
						"Warning: {}{} {} {}",
						"unusual table row (", cells.len(), "cells) in", url.to_string()
					);
					continue;
				}
				let link = cells[1].select(&LINKS).next().context("thread link not found")?;
				let object = Object::from_link(link, link)?;
				let mut path = path.clone();
				let name = format!(
					"{}_{}",
					object.url().thr_pk.as_ref().context("thr_pk not found for thread")?,
					link.text().collect::<String>().trim()
				);
				path.push(file_escape(&name));
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
				spawn!(process_gracefully(ilias, path, object));
			}
			if html.select(&forum_pages).count() > 0 {
				log!(0, "Ignoring older threads in {:?}..", path);
			}
		},
		Thread { url } => {
			if !ilias.opt.forum {
				return Ok(());
			}
			let mut all_images = Vec::new();
			let mut attachments = Vec::new();
			{
				let html = ilias.get_html(&url.url).await?;
				for post in html.select(&post_row) {
					let title = post
						.select(&post_title)
						.next()
						.context("post title not found")?
						.text()
						.collect::<String>();
					let author = post.select(&span_small).next().context("post author not found")?;
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
					let container = post.select(&post_container).next().context("post container not found")?;
					let link = container.select(&LINKS).next().context("post link not found")?;
					let id = link.value().attr("id").context("no id in thread link")?.to_owned();
					let name = format!("{}_{}_{}.html", id, author, title.trim());
					let data = container.inner_html();
					let path = path.join(file_escape(&name));
					let relative_path = relative_path.join(file_escape(&name));
					spawn!(handle_gracefully(async move {
						log!(0, "Writing {}", relative_path.display());
						write_file_data(&path, &mut data.as_bytes()).await.context("failed to write forum post")
					}));
					let images = container.select(&IMAGES).map(|x| x.value().attr("src").map(|x| x.to_owned()));
					for image in images {
						let image = image.context("no src on image")?;
						all_images.push((id.clone(), image));
					}
					if let Some(container) = container.select(&post_attachments).next() {
						for attachment in container.select(&LINKS) {
							attachments.push((
								id.clone(),
								attachment.text().collect::<String>(),
								attachment.value().attr("href").map(|x| x.to_owned()),
							));
						}
					}
				}
				// pagination
				if let Some(pages) = html.select(&TABLES).next() {
					if let Some(last) = pages.select(&links_in_table).last() {
						let text = last.text().collect::<String>();
						if text.trim() == ">>" {
							// not last page yet
							let ilias = Arc::clone(&ilias);
							let next_page = Thread {
								url: URL::from_href(last.value().attr("href").context("page link not found")?)?,
							};
							spawn!(process_gracefully(ilias, path.clone(), next_page));
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
				let mut path = path.clone();
				if let Some(m) = image_src_regex.captures(&image) {
					// image uploaded to ILIAS
					let (media_id, filename) = (m.get(1).unwrap().as_str(), m.get(2).unwrap().as_str());
					path.push(file_escape(&format!("{}_{}_{}", id, media_id, filename)));
				} else {
					// external image
					path.push(file_escape(&format!("{}_{}", id, image)));
				}
				spawn!(handle_gracefully(async move {
					let bytes = dl.bytes().await?;
					write_file_data(&path, &mut &*bytes)
						.await
						.context("failed to write forum post image attachment")
				}));
			}
			for (id, name, url) in attachments {
				let url = url.context("attachment without href")?;
				let src = URL::from_href(&url)?;
				let dl = ilias.download(&src.url).await?;
				let mut path = path.clone();
				path.push(file_escape(&format!("{}_{}", id, name)));
				spawn!(handle_gracefully(async move {
					let bytes = dl.bytes().await?;
					write_file_data(&path, &mut &*bytes)
						.await
						.context("failed to write forum post file attachment")
				}));
			}
		},
		ExerciseHandler { url, .. } => {
			let html = ilias.get_html(&url.url).await?;
			let mut filenames = HashSet::new();
			for row in html.select(&form_group) {
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
					.select(&form_name)
					.next()
					.context("link without file name")?
					.text()
					.collect::<String>()
					.trim()
					.to_owned();
				let item = File { url, name };
				let mut path = path.clone();
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
				spawn!(process_gracefully(ilias, path, item));
			}
		},
		Weblink { url, .. } => {
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
						.map(|(x, y)| URL::from_href(x).map(|z| (z, y.trim().to_owned())).context("parsing weblink"))
						.collect::<Result<Vec<_>>>()
				}?;

				for (url, name) in urls {
					if url.cmd.as_deref().unwrap_or("") != "callLink" {
						continue;
					}

					let head = ilias.head(url.url.as_str()).await.context("HEAD request to web link failed");
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
				write_file_data(&path, &mut url.as_bytes()).await.context("failed to save weblink URL")?;
			}
		},
		Wiki { .. } => {
			log!(1, "Ignored wiki!");
		},
		Survey { .. } => {
			log!(1, "Ignored survey!");
		},
		Presentation { .. } => {
			log!(1, "Ignored interactive presentation! (visit it yourself, it's probably interesting)");
		},
		Generic { .. } => {
			log!(1, "Ignored generic {:?}", obj)
		},
	}
	Ok(())
}
