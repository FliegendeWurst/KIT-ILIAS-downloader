#![allow(clippy::comparison_to_empty, clippy::upper_case_acronyms)]

use anyhow::{anyhow, Context, Result};
use colored::Colorize;
use futures::future::{self, Either};
use futures_channel::mpsc::UnboundedSender;
use futures_util::{stream::TryStreamExt, StreamExt};
use ignore::gitignore::Gitignore;
use lazy_static::lazy_static;
use parking_lot::Mutex;
use reqwest::{Client, Proxy};
use scraper::{ElementRef, Html, Selector};
use serde_json::json;
use structopt::StructOpt;
use tokio::fs;
use tokio::task::{self, JoinHandle};
use tokio_util::io::StreamReader;
use url::Url;

use std::future::Future;
use std::io;
use std::panic;
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;
use std::{collections::HashSet, default::Default, sync::atomic::AtomicUsize};

mod util;
use util::*;

const ILIAS_URL: &str = "https://ilias.studium.kit.edu/";

static LOG_LEVEL: AtomicUsize = AtomicUsize::new(0);

macro_rules! log {
	($lvl:expr, $($t:expr),+) => {
		#[allow(unused_comparisons)] // 0 <= 0
		if $lvl <= LOG_LEVEL.load(Ordering::SeqCst) {
			println!($($t),+);
		}
	}
}

macro_rules! info {
	($t:tt) => {
		println!($t);
	};
}

macro_rules! success {
	($t:tt) => {
		println!("{}", format!($t).bright_green());
	};
}

macro_rules! warning {
	($e:expr) => {
		println!("Warning: {}", format!("{:?}", $e).bright_yellow());
	};
	($msg:expr, $e:expr) => {
		println!("Warning: {}", format!("{} {:?}", $msg, $e).bright_yellow());
	};
	($msg1:expr, $msg2:expr, $e:expr) => {
		println!("Warning: {}", format!("{} {} {:?}", $msg1, $msg2, $e).bright_yellow());
	};
	(format => $($e:expr),+) => {
		println!("Warning: {}", format!($($e),+));
	};
}

macro_rules! error {
	($($prefix:expr),+; $e:expr) => {
		println!("{}: {}", format!($($prefix),+), format!("{:?}", $e).bright_red());
	};
	($e:expr) => {
		println!("Error: {}", format!("{:?}", $e).bright_red());
	};
}

#[tokio::main]
async fn main() {
	let mut opt = Opt::from_args();
	// use UNC paths on Windows
	opt.output = fs::canonicalize(opt.output).await.expect("failed to canonicalize directory");
	LOG_LEVEL.store(opt.verbose, Ordering::SeqCst);
	create_dir(&opt.output).await.expect("failed to create output directory");
	// need this because task scheduling is WIP
	// (would wait forever on paniced task)
	*PANIC_HOOK.lock() = panic::take_hook();
	panic::set_hook(Box::new(|info| {
		*TASKS_RUNNING.lock() -= 1;
		PANIC_HOOK.lock()(info);
	}));

	// load .iliasignore file
	opt.output.push(".iliasignore");
	let (ignore, error) = Gitignore::new(&opt.output);
	if let Some(err) = error {
		warning!(err);
	}
	opt.output.pop();
	// loac .iliaslogin file
	opt.output.push(".iliaslogin");
	let login = std::fs::read_to_string(&opt.output);
	let (user, pass) = if let Ok(login) = login {
		let mut lines = login.split('\n');
		let user = lines.next().expect("missing user in .iliaslogin");
		let pass = lines.next().expect("missing password in .iliaslogin");
		let user = user.trim();
		let pass = pass.trim();
		(user.to_owned(), pass.to_owned())
	} else {
		ask_user_pass()
	};
	opt.output.pop();

	let ilias = match ILIAS::login(opt, user, pass, ignore).await {
		Ok(ilias) => ilias,
		Err(e) => {
			error!(e);
			std::process::exit(77);
		},
	};
	if ilias.opt.content_tree {
		// need this to get the content tree
		if let Err(e) = ilias.client.get("https://ilias.studium.kit.edu/ilias.php?baseClass=ilRepositoryGUI&cmd=frameset&set_mode=tree&ref_id=1").send().await {
			warning!("could not enable content tree:", e);
		}
	}
	let ilias = Arc::new(ilias);
	let desktop = ilias.personal_desktop().await.context("Failed to load personal desktop");
	let (tx, mut rx) = futures_channel::mpsc::unbounded::<JoinHandle<()>>();
	*TASKS.lock() = Some(tx.clone());
	match desktop {
		Ok(desktop) => {
			for item in desktop.items {
				let mut path = ilias.opt.output.clone();
				path.push(file_escape(item.name()));
				let ilias = Arc::clone(&ilias);
				let _ = tx.unbounded_send(task::spawn(process_gracefully(ilias, path, item)));
			}
		},
		Err(e) => error!(e),
	}
	while let Either::Left((task, _)) = future::select(rx.next(), future::ready(())).await {
		if let Some(task) = task {
			let _ = task.await;
		} else {
			break;
		}
	}
	// channel is empty => all tasks are completed
	if ilias.opt.content_tree {
		// restore fast page loading times
		if let Err(e) = ilias.client.get("https://ilias.studium.kit.edu/ilias.php?baseClass=ilRepositoryGUI&cmd=frameset&set_mode=flat&ref_id=1").send().await {
			warning!("could not disable content tree:", e);
		}
	}
}

lazy_static! {
	static ref TASKS: Mutex<Option<UnboundedSender<JoinHandle<()>>>> = Mutex::default();
	static ref TASKS_RUNNING: Mutex<usize> = Mutex::default();
	static ref PANIC_HOOK: Mutex<Box<dyn Fn(&panic::PanicInfo) + Sync + Send + 'static>> = Mutex::new(Box::new(|_| {}));
}

macro_rules! spawn {
	($e:expr) => {
		let _ = TASKS.lock().as_ref().unwrap().unbounded_send(task::spawn($e));
	};
}

fn ask_user_pass() -> (String, String) {
	let user = rprompt::prompt_reply_stdout("Username: ").expect("username prompt");
	let pass = rpassword::read_password_from_tty(Some("Password: ")).expect("password prompt");
	(user, pass)
}

// https://github.com/rust-lang/rust/issues/53690#issuecomment-418911229
#[allow(clippy::manual_async_fn)]
fn process_gracefully(
	ilias: Arc<ILIAS>,
	path: PathBuf,
	obj: Object,
) -> impl Future<Output = ()> + Send { async move {
	loop {
		{
			// limit scope of lock
			let mut running = TASKS_RUNNING.lock();
			if *running < ilias.opt.jobs {
				*running += 1;
				break;
			}
		}
		tokio::time::sleep(Duration::from_millis(50)).await;
	}
	let path_text = path.to_string_lossy().into_owned();
	if let Err(e) = process(ilias, path, obj).await.context("failed to process URL") {
		error!("Syncing {}", path_text; e);
	}
	*TASKS_RUNNING.lock() -= 1;
}}

async fn handle_gracefully(fut: impl Future<Output = Result<()>>) {
	if let Err(e) = fut.await {
		error!(e);
	}
}

#[allow(non_upper_case_globals)]
mod selectors {
	use lazy_static::lazy_static;
	use regex::Regex;
	use scraper::Selector;
	// construct CSS selectors once
	lazy_static! {
		pub static ref a: Selector = Selector::parse("a").unwrap();
		pub static ref a_target_blank: Selector = Selector::parse(r#"a[target="_blank"]"#).unwrap();
		pub static ref img: Selector = Selector::parse("img").unwrap();
		pub static ref table: Selector = Selector::parse("table").unwrap();
		pub static ref video_tr: Selector = Selector::parse(".ilTableOuter > div > table > tbody > tr").unwrap();
		pub static ref links_in_table: Selector = Selector::parse("tbody tr td a").unwrap();
		pub static ref td: Selector = Selector::parse("td").unwrap();
		pub static ref tr: Selector = Selector::parse("tr").unwrap();
		pub static ref post_row: Selector = Selector::parse(".ilFrmPostRow").unwrap();
		pub static ref post_title: Selector = Selector::parse(".ilFrmPostTitle").unwrap();
		pub static ref post_container: Selector = Selector::parse(".ilFrmPostContentContainer").unwrap();
		pub static ref post_attachments: Selector = Selector::parse(".ilFrmPostAttachmentsContainer").unwrap();
		pub static ref span_small: Selector = Selector::parse("span.small").unwrap();
		pub static ref forum_pages: Selector = Selector::parse("div.ilTableNav > table > tbody > tr > td > a").unwrap();
		pub static ref alert_danger: Selector = Selector::parse("div.alert-danger").unwrap();
		pub static ref tree_highlighted: Selector = Selector::parse("span.ilHighlighted").unwrap();
		pub static ref form_group: Selector = Selector::parse(".form-group").unwrap();
		pub static ref form_name: Selector = Selector::parse(".il_InfoScreenProperty").unwrap();
		pub static ref cmd_node_regex: Regex = Regex::new(r#"cmdNode=uf:\w\w"#).unwrap();
		pub static ref image_src_regex: Regex = Regex::new(r#"\./data/produktiv/mobs/mm_(\d+)/([^?]+).+"#).unwrap();
		pub static ref XOCT_REGEX: Regex = Regex::new(r#"(?m)<script>\s+xoctPaellaPlayer\.init\(([\s\S]+)\)\s+</script>"#).unwrap();
	}
}
use crate::selectors::*;

async fn process(ilias: Arc<ILIAS>, mut path: PathBuf, obj: Object) -> Result<()> {
	let relative_path = path.strip_prefix(&ilias.opt.output).unwrap();
	if ilias.ignore.matched(relative_path, obj.is_dir()).is_ignore() {
		log!(1, "Ignored {}", relative_path.to_string_lossy());
		return Ok(());
	}
	log!(1, "Syncing {} {}", obj.kind(), relative_path.to_string_lossy());
	log!(2, " URL: {}", obj.url().url);
	match &obj {
		Course { url, name } => {
			create_dir(&path).await?;
			let content = if ilias.opt.content_tree {
				let html = ilias.download(&url.url).await?.text().await?;
				let cmd_node = cmd_node_regex.find(&html).context("can't find cmdNode")?.as_str()[8..].to_owned();
				let content_tree = ilias.get_course_content_tree(&url.ref_id, &cmd_node).await;
				match content_tree {
					Ok(tree) => tree,
					Err(e) => {
						// some folders are hidden on the course page and can only be found via the RSS feed / recent activity / content tree sidebar
						// TODO: this is probably never the case for folders?
						if html.contains(r#"input[name="cmd[join]""#) {
							return Ok(()); // ignore groups we are not in
						}
						warning!(name, "falling back to incomplete course content extractor!", e);
						ilias.get_course_content(&url).await?.into_iter().flat_map(Result::ok).collect() // TODO: perhaps don't download almost the same content 3x
					},
				}
			} else {
				ilias.get_course_content(&url).await?.into_iter().flat_map(Result::ok).collect()
			};
			for item in content {
				let mut path = path.clone();
				path.push(file_escape(item.name()));
				let ilias = Arc::clone(&ilias);
				spawn!(process_gracefully(ilias, path, item));
			}
		},
		Folder { url, .. } => {
			create_dir(&path).await?;
			let content = ilias.get_course_content(&url).await?;
			for item in content {
				if item.is_err() {
					log!(1, "Ignoring: {:?}", item.err().unwrap());
					continue;
				}
				let item = item.unwrap();
				let mut path = path.clone();
				path.push(file_escape(item.name()));
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
			let mut reader = StreamReader::new(data.bytes_stream().map_err(|x| {
				io::Error::new(io::ErrorKind::Other, x)
			}));
			log!(0, "Writing {}", relative_path.to_string_lossy());
			write_file_data(&path, &mut reader).await?;
		},
		PluginDispatch { url, .. } => {
			if ilias.opt.no_videos {
				return Ok(());
			}
			create_dir(&path).await?;
			let full_url = {
				// first find the link to full video list
				let list_url = format!("{}ilias.php?ref_id={}&cmdClass=xocteventgui&cmdNode=nc:n4:14u&baseClass=ilObjPluginDispatchGUI&lang=de&limit=20&cmd=asyncGetTableGUI&cmdMode=asynch", ILIAS_URL, url.ref_id);
				log!(1, "Loading {}", list_url);
				let data = ilias.download(&list_url).await?;
				let html = data.text().await?;
				let html = Html::parse_fragment(&html);
				html.select(&a)
					.filter_map(|link| link.value().attr("href"))
					.filter(|href| href.contains("trows=800"))
					.map(|x| x.to_string()).next().context("video list link not found")?
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
			let data = ilias.download(&full_url.into_string()).await?;
			let html = data.text().await?;
			let html = Html::parse_fragment(&html);
			for row in html.select(&video_tr) {
				let link = row.select(&a_target_blank).next();
				if link.is_none() {
					if !row.text().any(|x| x == "Keine Einträge") {
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
						url: URL::raw(
							link.value()
								.attr("href")
								.context("video link without href")?
								.to_owned(),
						),
					};
					let ilias = Arc::clone(&ilias);
					spawn!(async {
						process_gracefully(ilias, path, video).await;
					});
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
				let json = json
					.split(",\n")
					.next()
					.context("invalid xoct player json")?;
				serde_json::from_str(&json.trim())?
			};
			log!(2, "{}", json);
			let url = json
				.pointer("/streams/0/sources/mp4/0/src")
				.map(|x| x.as_str())
				.context("video src not found")?
				.context("video src not string")?;
			let meta = fs::metadata(&path).await;
			if !ilias.opt.force && meta.is_ok() && ilias.opt.check_videos {
				let head = ilias
					.client
					.head(url)
					.send()
					.await
					.context("HEAD request failed")?;
				if let Some(len) = head.headers().get("content-length") {
					if meta.unwrap().len() != len.to_str()?.parse::<u64>()? {
						warning!(relative_path.to_string_lossy(), "was updated, consider moving the outdated file");
					}
				}
			} else {
				let resp = ilias.download(&url).await?;
				let mut reader = StreamReader::new(
				resp.bytes_stream()
						.map_err(|x| io::Error::new(io::ErrorKind::Other, x)),
				);
				log!(0, "Writing {}", relative_path.to_string_lossy());
				write_file_data(&path, &mut reader).await?;
			}
		},
		Forum { url, .. } => {
			if !ilias.opt.forum {
				return Ok(());
			}
			create_dir(&path).await?;
			let url = &url.url;
			let html = {
				let data = ilias.download(url);
				let html_text = data.await?.text().await?;
				let url = {
					let html = Html::parse_document(&html_text);
					//https://ilias.studium.kit.edu/ilias.php?ref_id=122&cmdClass=ilobjforumgui&frm_tt_e39_122_trows=800&cmd=showThreads&cmdNode=uf:lg&baseClass=ilrepositorygui
					html.select(&a)
						.flat_map(|x| x.value().attr("href"))
						.find(|x| x.contains("trows=800"))
						.context("can't find forum thread count selector (empty forum?)")?
						.to_owned()
				};
				let data = ilias.download(&url);
				let html = data.await?.text().await?;
				Html::parse_document(&html)
			};
			for row in html.select(&tr) {
				let cells = row.select(&td).collect::<Vec<_>>();
				if cells.len() != 6 {
					log!(
						0,
						"Warning: {}{} {} {}",
						"unusual table row (".bright_yellow(),
						cells.len().to_string().bright_yellow(),
						"cells) in".bright_yellow(),
						url.to_string().bright_yellow()
					);
					continue;
				}
				let link = cells[1]
					.select(&a)
					.next()
					.context("thread link not found")?;
				let object = Object::from_link(link, link)?;
				let mut path = path.clone();
				let name = format!(
					"{}_{}",
					object
						.url()
						.thr_pk
						.as_ref()
						.context("thr_pk not found for thread")?,
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
				log!(0, "New posts in {:?}..", path);
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
			create_dir(&path).await?;
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
					let author = post
						.select(&span_small)
						.next()
						.context("post author not found")?;
					let author = author.text().collect::<String>();
					let author = author
						.trim()
						.split('|')
						.nth(1)
						.context("author data in unknown format")?
						.trim();
					let container = post
						.select(&post_container)
						.next()
						.context("post container not found")?;
					let link = container.select(&a).next().context("post link not found")?;
					let id = link
						.value()
						.attr("id")
						.context("no id in thread link")?
						.to_owned();
					let name = format!("{}_{}_{}.html", id, author, title.trim());
					let data = container.inner_html();
					let mut path = path.clone();
					path.push(file_escape(&name));
					spawn!(handle_gracefully(async move {
						write_file_data(&path, &mut data.as_bytes())
							.await
							.context("failed to write forum post")
					}));
					let images = container
						.select(&img)
						.map(|x| x.value().attr("src").map(|x| x.to_owned()));
					for image in images {
						let image = image.context("no src on image")?;
						all_images.push((id.clone(), image));
					}
					if let Some(container) = container.select(&post_attachments).next() {
						for attachment in container.select(&a) {
							attachments.push((
								id.clone(),
								attachment.text().collect::<String>(),
								attachment.value().attr("href").map(|x| x.to_owned()),
							));
						}
					}
				}
				// pagination
				if let Some(pages) = html.select(&table).next() {
					if let Some(last) = pages.select(&links_in_table).last() {
						let text = last.text().collect::<String>();
						if text.trim() == ">>" {
							// not last page yet
							let ilias = Arc::clone(&ilias);
							let next_page = Thread {
								url: URL::from_href(
									last.value().attr("href").context("page link not found")?,
								)?,
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
				let m = image_src_regex
					.captures(&image)
					.context(format!("image src {} unexpected format", image))?;
				let (media_id, filename) = (m.get(1).unwrap().as_str(), m.get(2).unwrap().as_str());
				let mut path = path.clone();
				path.push(file_escape(&format!("{}_{}_{}", id, media_id, filename)));
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
			create_dir(&path).await?;
			let html = ilias.get_html(&url.url).await?;
			let mut filenames = HashSet::new();
			for row in html.select(&form_group) {
				let link = row.select(&a).next();
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
				if cmd != "downloadFile"
					&& cmd != "downloadGlobalFeedbackFile"
					&& cmd != "downloadFeedbackFile"
				{
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
					if name != "" {
						unique_filename = format!("{}{}.{}", name, i, extension);
					} else {
						unique_filename = format!("{}{}", extension, i);
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
			let head_req_result = ilias.client.head(&url.url).send().await;
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
					html.select(&a)
						.filter_map(|x| {
							x.value()
								.attr("href")
								.map(|y| (y, x.text().collect::<String>()))
						})
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

					let head = ilias.client.head(url.url.as_str()).send().await.context("HEAD request to web link failed");
					if let Some(err) = head.as_ref().err() {
						warning!(err);
						continue;
					}
					let head = head.unwrap();
					let url = head.url().as_str();
					path.push(file_escape(&name));
					write_file_data(&path, &mut url.as_bytes()).await?;
					path.pop();
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

#[derive(Debug, StructOpt)]
#[structopt(name = env!("CARGO_PKG_NAME"))]
struct Opt {
	/// Do not download files
	#[structopt(short, long)]
	skip_files: bool,

	/// Do not download Opencast videos
	#[structopt(short, long)]
	no_videos: bool,

	/// Download forum content
	#[structopt(short = "t", long)]
	forum: bool,

	/// Re-download already present files
	#[structopt(short)]
	force: bool,

	/// Use content tree (slow but thorough)
	#[structopt(long)]
	content_tree: bool,

	/// Re-check OpenCast lectures (slow)
	#[structopt(long)]
	check_videos: bool,

	/// Verbose logging
	#[structopt(short, multiple = true, parse(from_occurrences))]
	verbose: usize,

	/// Output directory
	#[structopt(short, long, parse(from_os_str))]
	output: PathBuf,

	/// Parallel download jobs
	#[structopt(short, long, default_value = "1")]
	jobs: usize,

	/// Proxy, e.g. socks5h://127.0.0.1:1080
	#[structopt(short, long)]
	proxy: Option<String>,
}

struct ILIAS {
	opt: Opt,
	ignore: Gitignore,
	// TODO: use these for re-authentication in case of session timeout/invalidation
	user: String,
	pass: String,
	client: Client,
}

impl ILIAS {
	async fn login(opt: Opt, user: impl Into<String>, pass: impl Into<String>, ignore: Gitignore) -> Result<Self> {
		let user = user.into();
		let pass = pass.into();
		let mut builder = Client::builder()
			.cookie_store(true)
			.user_agent(concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION")));
		if let Some(proxy) = opt.proxy.as_ref() {
			let proxy = Proxy::all(proxy)?;
			builder = builder.proxy(proxy);
		}
		let client = builder
			// timeout is infinite by default
			.build()?;
		let this = ILIAS { opt, ignore, user, pass, client };
		info!("Logging into ILIAS using KIT account..");
		let session_establishment = this.client
			.post("https://ilias.studium.kit.edu/Shibboleth.sso/Login")
			.form(&json!({
				"sendLogin": "1",
				"idp_selection": "https://idp.scc.kit.edu/idp/shibboleth",
				"target": "/shib_login.php?target=",
				"home_organization_selection": "Mit KIT-Account anmelden"
			}))
			.send().await?;
		let url = session_establishment.url().clone();
		let text = session_establishment.text().await?;
		let dom_sso = Html::parse_document(text.as_str());
		let csrf_token = dom_sso
			.select(&Selector::parse(r#"input[name="csrf_token"]"#).unwrap())
			.next().context("no csrf token")?;
		info!("Logging into Shibboleth..");
		let login_response = this.client
			.post(url)
			.form(&json!({
				"j_username": &this.user,
				"j_password": &this.pass,
				"_eventId_proceed": "",
				"csrf_token": csrf_token.value().attr("value").context("no csrf token")?,
			}))
			.send().await?
			.text().await?;
		let dom = Html::parse_document(&login_response);
		let saml = Selector::parse(r#"input[name="SAMLResponse"]"#).unwrap();
		let saml = dom
			.select(&saml)
			.next().context("no SAML response, incorrect password?")?;
		let relay_state = Selector::parse(r#"input[name="RelayState"]"#).unwrap();
		let relay_state = dom.select(&relay_state).next().context("no relay state")?;
		info!("Logging into ILIAS..");
		this.client
			.post("https://ilias.studium.kit.edu/Shibboleth.sso/SAML2/POST")
			.form(&json!({
				"SAMLResponse": saml.value().attr("value").context("no SAML value")?,
				"RelayState": relay_state.value().attr("value").context("no RelayState value")?
			}))
			.send().await?;
		success!("Logged in!");
		Ok(this)
	}

	async fn download(&self, url: &str) -> Result<reqwest::Response> {
		log!(2, "Downloading {}", url);
		if url.starts_with("http") || url.starts_with("ilias.studium.kit.edu") {
			Ok(self.client.get(url).send().await?)
		} else {
			Ok(self.client.get(&format!("{}{}", ILIAS_URL, url)).send().await?)
		}
	}

	async fn get_html(&self, url: &str) -> Result<Html> {
		let text = self.download(url).await?.text().await?;
		let html = Html::parse_document(&text);
		if html.select(&alert_danger).next().is_some() {
			Err(anyhow!("ILIAS error"))
		} else {
			Ok(html)
		}
	}

	async fn get_html_fragment(&self, url: &str) -> Result<Html> {
		let text = self.download(url).await?.text().await?;
		let html = Html::parse_fragment(&text);
		if html.select(&alert_danger).next().is_some() {
			Err(anyhow!("ILIAS error"))
		} else {
			Ok(html)
		}
	}

	fn get_items(html: &Html) -> Vec<Result<Object>> {
		let container_items = Selector::parse("div.il_ContainerListItem").unwrap();
		let container_item_title = Selector::parse("a.il_ContainerItemTitle").unwrap();
		html.select(&container_items)
			.map(|item| {
				item.select(&container_item_title)
					.next()
					.map(|link| Object::from_link(item, link))
					.context("can't find link").flatten2()
			})
			.collect()
	}

	async fn get_course_content(&self, url: &URL) -> Result<Vec<Result<Object>>> {
		let html = self.get_html(&url.url).await?;
		Ok(ILIAS::get_items(&html))
	}

	async fn personal_desktop(&self) -> Result<Dashboard> {
		let html = self.get_html("https://ilias.studium.kit.edu/ilias.php?baseClass=ilPersonalDesktopGUI&cmd=jumpToSelectedItems").await?;
		let items = ILIAS::get_items(&html)
			.into_iter()
			.flat_map(Result::ok)
			.collect();
		Ok(Dashboard { items })
	}

	async fn get_course_content_tree(&self, ref_id: &str, cmd_node: &str) -> Result<Vec<Object>> {
		// TODO: this magically does not return sub-folders
		// opening the same url in browser does show sub-folders?!
		let url = format!(
			"{}ilias.php?ref_id={}&cmdClass=ilobjcoursegui&cmd=showRepTree&cmdNode={}&baseClass=ilRepositoryGUI&cmdMode=asynch&exp_cmd=getNodeAsync&node_id=exp_node_rep_exp_{}&exp_cont=il_expl2_jstree_cont_rep_exp&searchterm=",
			ILIAS_URL, ref_id, cmd_node, ref_id
		);
		let html = self.get_html_fragment(&url).await?;
		let mut items = Vec::new();
		for link in html.select(&a) {
			if link.value().attr("href").is_some() {
				items.push(Object::from_link(link, link)?);
			} // else: disabled course
		}
		Ok(items)
	}
}

#[derive(Debug)]
struct Dashboard {
	items: Vec<Object>,
}

#[derive(Debug)]
enum Object {
	Course { name: String, url: URL },
	Folder { name: String, url: URL },
	File { name: String, url: URL },
	Forum { name: String, url: URL },
	Thread { url: URL },
	Wiki { name: String, url: URL },
	ExerciseHandler { name: String, url: URL },
	Weblink { name: String, url: URL },
	Survey { name: String, url: URL },
	Presentation { name: String, url: URL },
	PluginDispatch { name: String, url: URL },
	Video { url: URL },
	Generic { name: String, url: URL },
}

use Object::*;

impl Object {
	fn name(&self) -> &str {
		match self {
			Course { name, .. }
			| Folder { name, .. }
			| File { name, .. }
			| Forum { name, .. }
			| Wiki { name, .. }
			| Weblink { name, .. }
			| Survey { name, .. }
			| Presentation { name, .. }
			| ExerciseHandler { name, .. }
			| PluginDispatch { name, .. }
			| Generic { name, .. } => &name,
			Thread { url } => &url.thr_pk.as_ref().unwrap(),
			Video { url } => &url.url,
		}
	}

	fn url(&self) -> &URL {
		match self {
			Course { url, .. }
			| Folder { url, .. }
			| File { url, .. }
			| Forum { url, .. }
			| Thread { url }
			| Wiki { url, .. }
			| Weblink { url, .. }
			| Survey { url, .. }
			| Presentation { url, .. }
			| ExerciseHandler { url, .. }
			| PluginDispatch { url, .. }
			| Video { url }
			| Generic { url, .. } => &url,
		}
	}

	fn kind(&self) -> &str {
		match self {
			Course { .. } => "course",
			Folder { .. } => "folder",
			File { .. } => "file",
			Forum { .. } => "forum",
			Thread { .. } => "thread",
			Wiki { .. } => "wiki",
			Weblink { .. } => "weblink",
			Survey { .. } => "survey",
			Presentation { .. } => "presentation",
			ExerciseHandler { .. } => "exercise handler",
			PluginDispatch { .. } => "plugin dispatch",
			Video { .. } => "video",
			Generic { .. } => "generic",
		}
	}

	fn is_dir(&self) -> bool {
		match self {
			Course { .. }
			| Folder { .. }
			| Forum { .. }
			| Thread { .. }
			| Wiki { .. }
			| ExerciseHandler { .. }
			| Presentation { .. }
			| PluginDispatch { .. } => true,
			File { .. } | Video { .. } | Weblink { .. } | Survey { .. } | Generic { .. } => false,
		}
	}

	fn from_link(item: ElementRef, link: ElementRef) -> Result<Self> {
		let mut name = link
			.text()
			.collect::<String>()
			.replace('/', "-")
			.trim()
			.to_owned();
		let mut url = URL::from_href(link.value().attr("href").context("link missing href")?)?;

		if url.thr_pk.is_some() {
			return Ok(Thread { url });
		}

		if url
			.url
			.starts_with("https://ilias.studium.kit.edu/goto.php")
		{
			let target = url.target.as_deref().unwrap_or("NONE");
			if target.starts_with("wiki_") {
				return Ok(Wiki {
					name,
					url, // TODO: insert ref_id here
				});
			}
			if target.starts_with("root_") {
				// magazine link
				return Ok(Generic { name, url });
			}
			if target.starts_with("crs_") {
				let ref_id = url.target.as_ref().unwrap().split('_').nth(1).unwrap();
				url.ref_id = ref_id.to_owned();
				return Ok(Course { name, url });
			}
			if target.starts_with("frm_") {
				// TODO: extract post link? (this codepath should only be hit when parsing the content tree)
				let ref_id = url.target.as_ref().unwrap().split('_').nth(1).unwrap();
				url.ref_id = ref_id.to_owned();
				return Ok(Forum { name, url });
			}
			if target.starts_with("lm_") {
				// fancy interactive task
				return Ok(Presentation { name, url });
			}
			if target.starts_with("fold_") {
				let ref_id = url.target.as_ref().unwrap().split('_').nth(1).unwrap();
				url.ref_id = ref_id.to_owned();
				return Ok(Folder { name, url });
			}
			if target.starts_with("file_") {
				if !target.ends_with("download") {
					// download page containing metadata
					return Ok(Generic { name, url });
				} else {
					let item_prop = Selector::parse("span.il_ItemProperty").unwrap();
					let mut item_props = item.select(&item_prop);
					let ext = item_props.next().context("cannot find file extension")?;
					let version = item_props
						.nth(1)
						.context("cannot find 3rd file metadata")?
						.text()
						.collect::<String>();
					let version = version.trim();
					if let Some(v) = version.strip_prefix("Version: ") {
						name += "_v";
						name += v;
					}
					return Ok(File {
						name: format!("{}.{}", name, ext.text().collect::<String>().trim()),
						url,
					});
				}
			}
			return Ok(Generic { name, url });
		}

		if url.cmd.as_deref() == Some("showThreads") {
			return Ok(Forum { name, url });
		}

		// class name is *sometimes* in CamelCase
		Ok(match &*url.baseClass.to_ascii_lowercase() {
			"ilexercisehandlergui" => ExerciseHandler { name, url },
			"ililwikihandlergui" => Wiki { name, url },
			"illinkresourcehandlergui" => Weblink { name, url },
			"ilobjsurveygui" => Survey { name, url },
			"illmpresentationgui" => Presentation { name, url },
			"ilrepositorygui" => match url.cmd.as_deref() {
				Some("view") | Some("render") => Folder { name, url },
				Some(_) => Generic { name, url },
				None => Course { name, url },
			},
			"ilobjplugindispatchgui" => PluginDispatch { name, url },
			_ => Generic { name, url },
		})
	}
}

#[allow(non_snake_case)]
#[derive(Debug)]
struct URL {
	url: String,
	baseClass: String,
	cmdClass: Option<String>,
	cmdNode: Option<String>,
	cmd: Option<String>,
	forwardCmd: Option<String>,
	thr_pk: Option<String>,
	pos_pk: Option<String>,
	ref_id: String,
	target: Option<String>,
	file: Option<String>,
}

#[allow(non_snake_case)]
impl URL {
	fn raw(url: String) -> Self {
		URL {
			url,
			baseClass: String::new(),
			cmdClass: None,
			cmdNode: None,
			cmd: None,
			forwardCmd: None,
			thr_pk: None,
			pos_pk: None,
			ref_id: String::new(),
			target: None,
			file: None,
		}
	}

	fn from_href(href: &str) -> Result<Self> {
		let url = if !href.starts_with(ILIAS_URL) {
			Url::parse(&format!("{}{}", ILIAS_URL, href))?
		} else {
			Url::parse(href)?
		};
		let mut baseClass = String::new();
		let mut cmdClass = None;
		let mut cmdNode = None;
		let mut cmd = None;
		let mut forwardCmd = None;
		let mut thr_pk = None;
		let mut pos_pk = None;
		let mut ref_id = String::new();
		let mut target = None;
		let mut file = None;
		for (k, v) in url.query_pairs() {
			match &*k {
				"baseClass" => baseClass = v.into_owned(),
				"cmdClass" => cmdClass = Some(v.into_owned()),
				"cmdNode" => cmdNode = Some(v.into_owned()),
				"cmd" => cmd = Some(v.into_owned()),
				"forwardCmd" => forwardCmd = Some(v.into_owned()),
				"thr_pk" => thr_pk = Some(v.into_owned()),
				"pos_pk" => pos_pk = Some(v.into_owned()),
				"ref_id" => ref_id = v.into_owned(),
				"target" => target = Some(v.into_owned()),
				"file" => file = Some(v.into_owned()),
				_ => {},
			}
		}
		Ok(URL {
			url: url.into_string(),
			baseClass,
			cmdClass,
			cmdNode,
			cmd,
			forwardCmd,
			thr_pk,
			pos_pk,
			ref_id,
			target,
			file,
		})
	}
}

#[cfg(not(target_os = "windows"))]
const INVALID: &[char] = &['/', '\\'];
#[cfg(target_os = "windows")]
const INVALID: &[char] = &['/', '\\', ':', '<', '>', '"', '|', '?', '*'];

fn file_escape(s: &str) -> String {
	s.replace(INVALID, "-")
}
