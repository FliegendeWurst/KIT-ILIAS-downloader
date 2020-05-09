use anyhow::{Context, Result, anyhow};
use futures_util::stream::TryStreamExt;
use ignore::gitignore::Gitignore;
use lazy_static::lazy_static;
use parking_lot::Mutex;
use regex::Regex;
use reqwest::Client;
use scraper::{ElementRef, Html, Selector};
use serde_json::json;
use structopt::StructOpt;
use tokio::fs::File as AsyncFile;
use tokio::io::{stream_reader, BufWriter};
use tokio::task;
use url::Url;

use std::default::Default;
use std::fs;
use std::io;
use std::panic;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

const ILIAS_URL: &'static str = "https://ilias.studium.kit.edu/";

#[tokio::main]
async fn main() {
	let opt = Opt::from_args();
	// need this because task scheduling is WIP
	// (would wait forever on paniced task)
	*PANIC_HOOK.lock() = panic::take_hook();
	panic::set_hook(Box::new(|info| {
		*TASKS_RUNNING.lock() -= 1;
		*TASKS_QUEUED.lock() -= 1;
		PANIC_HOOK.lock()(info);
	}));

	let user = rprompt::prompt_reply_stdout("Username: ").expect("username prompt");
	let pass = rpassword::read_password_from_tty(Some("Password: ")).expect("password prompt");
	let ilias = match ILIAS::login(opt, user, pass).await {
		Ok(ilias) => ilias,
		Err(e) => {
			print!("{:?}", e);
			std::process::exit(77);
		}
	};
	if ilias.opt.content_tree {
		// need this to get the content tree
		if let Err(e) = ilias.client.get("https://ilias.studium.kit.edu/ilias.php?baseClass=ilRepositoryGUI&cmd=frameset&set_mode=tree&ref_id=1").send().await {
			println!("Warning: could not enable content tree: {:?}", e);
		}
	}
	let ilias = Arc::new(ilias);
	let desktop = ilias.personal_desktop().await.context("Failed to load personal desktop");
	match desktop {
		Ok(desktop) => {
			for item in desktop.items {
				let mut path = ilias.opt.output.clone();
				path.push(item.name());
				let ilias = Arc::clone(&ilias);
				task::spawn(async {
					process_gracefully(ilias, path, item).await;
				});
			}
		},
		Err(e) => println!("{:?}", e)
	}
	// TODO: do this with tokio
	// https://github.com/tokio-rs/tokio/issues/2039
	while *TASKS_QUEUED.lock() > 0 {
		tokio::time::delay_for(Duration::from_millis(500)).await;
	}
	if ilias.opt.content_tree {
		// restore fast page loading times
		if let Err(e) = ilias.client.get("https://ilias.studium.kit.edu/ilias.php?baseClass=ilRepositoryGUI&cmd=frameset&set_mode=flat&ref_id=1").send().await {
			println!("Warning: could not disable content tree: {:?}", e);
		}
	}
}

lazy_static!{
	static ref TASKS_QUEUED: Mutex<usize> = Mutex::default();
	static ref TASKS_RUNNING: Mutex<usize> = Mutex::default();

	static ref PANIC_HOOK: Mutex<Box<dyn Fn(&panic::PanicInfo) + Sync + Send + 'static>> = Mutex::new(Box::new(|_| {}));
}

fn process_gracefully(ilias: Arc<ILIAS>, path: PathBuf, obj: Object) -> impl std::future::Future<Output = ()> + Send { async move {
	*TASKS_QUEUED.lock() += 1;
	while *TASKS_RUNNING.lock() >= ilias.opt.jobs {
		tokio::time::delay_for(Duration::from_millis(100)).await;
	}
	*TASKS_RUNNING.lock() += 1;
	let path_text = format!("{:?}", path);
	if let Err(e) = process(ilias, path, obj).await.context("Failed to process URL") {
		println!("Syncing {}: {:?}", path_text, e);
	}
	*TASKS_RUNNING.lock() -= 1;
	*TASKS_QUEUED.lock() -= 1;
}}

#[allow(non_upper_case_globals)]
mod selectors {
	use lazy_static::lazy_static;
	use regex::Regex;
	use scraper::Selector;
	// construct CSS selectors once
	lazy_static!{
		pub static ref a: Selector = Selector::parse("a").unwrap();
		pub static ref a_target_blank: Selector = Selector::parse(r#"a[target="_blank"]"#).unwrap();
		pub static ref table: Selector = Selector::parse("table").unwrap();
		pub static ref video_tr: Selector = Selector::parse(".ilTableOuter > div > table > tbody > tr").unwrap();
		pub static ref links_in_table: Selector = Selector::parse("tbody tr td a").unwrap();
		pub static ref td: Selector = Selector::parse("td").unwrap();
		pub static ref tr: Selector = Selector::parse("tr").unwrap();
		pub static ref post_row: Selector = Selector::parse(".ilFrmPostRow").unwrap();
		pub static ref post_title: Selector = Selector::parse(".ilFrmPostTitle").unwrap();
		pub static ref post_container: Selector = Selector::parse(".ilFrmPostContentContainer").unwrap();
		pub static ref post_content: Selector = Selector::parse(".ilFrmPostContent").unwrap();
		pub static ref span_small: Selector = Selector::parse("span.small").unwrap();
		pub static ref forum_pages: Selector = Selector::parse("div.ilTableNav > table > tbody > tr > td > a").unwrap();
		pub static ref alert_danger: Selector = Selector::parse("div.alert-danger").unwrap();
		pub static ref tree_highlighted: Selector = Selector::parse("span.ilHighlighted").unwrap();

		pub static ref cmd_node_regex: Regex = Regex::new(r#"cmdNode=uf:\w\w"#).unwrap();
	}
}
use crate::selectors::*;

// see https://github.com/rust-lang/rust/issues/53690#issuecomment-418911229
//async fn process(ilias: Arc<ILIAS>, path: PathBuf, obj: Object) {
fn process(ilias: Arc<ILIAS>, mut path: PathBuf, obj: Object) -> impl std::future::Future<Output = Result<()>> + Send { async move {
	let log_level = ilias.opt.verbose;
	macro_rules! log {
		($lvl:expr, $($arg:expr),*) => {
			#[allow(unused_comparisons)] // 0 >= 0
			if log_level >= $lvl {
				println!($($arg),*);
			}
		}
	}

	let relative_path = path.strip_prefix(&ilias.opt.output).unwrap();
	if ilias.ignore.matched(relative_path, obj.is_dir()).is_ignore() {
		log!(1, "Ignored {}", relative_path.to_string_lossy());
		return Ok(());
	}
	log!(1, "Syncing {} {}", obj.kind(), relative_path.to_string_lossy());
	log!(2, " URL: {}", obj.url().url);
	match &obj {
		Course { url, name } => {
			if let Err(e) = fs::create_dir(&path) {
				if e.kind() != io::ErrorKind::AlreadyExists {
					Err(e)?;
				}
			}
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
						log!(0, "Warning: {:?} falling back to incomplete course content extractor! {:?}", name, e);
						ilias.get_course_content(&url).await?.into_iter().flat_map(Result::ok).collect() // TODO: perhaps don't download almost the same content 3x
					}
				}
			} else {
				ilias.get_course_content(&url).await?.into_iter().flat_map(Result::ok).collect()
			};
			for item in content {
				let mut path = path.clone();
				path.push(item.name());
				let ilias = Arc::clone(&ilias);
				task::spawn(async {
					process_gracefully(ilias, path, item).await;
				});
			}
		},
		Folder { url, .. } => {
			if let Err(e) = fs::create_dir(&path) {
				if e.kind() != io::ErrorKind::AlreadyExists {
					Err(e)?;
				}
			}
			let content = ilias.get_course_content(&url).await?;
			for item in content {
				if item.is_err() {
					log!(1, "Ignoring: {:?}", item.err().unwrap());
					continue;
				}
				let item = item.unwrap();
				let mut path = path.clone();
				path.push(item.name());
				let ilias = Arc::clone(&ilias);
				task::spawn(async {
					process_gracefully(ilias, path, item).await;
				});
			}
		},
		File { url, .. } => {
			if ilias.opt.skip_files {
				return Ok(());
			}
			if !ilias.opt.force && fs::metadata(&path).is_ok() {
				log!(2, "Skipping download, file exists already");
				return Ok(());
			}
			let data = ilias.download(&url.url).await?;
			let mut reader = stream_reader(data.bytes_stream().map_err(|x| {
				io::Error::new(io::ErrorKind::Other, x)
			}));
			log!(0, "Writing {}", relative_path.to_string_lossy());
			let file = AsyncFile::create(&path).await?;
			let mut file = BufWriter::new(file);
			tokio::io::copy(&mut reader, &mut file).await?;
		},
		PluginDispatch { url, .. } => {
			if ilias.opt.no_videos {
				return Ok(());
			}
			if let Err(e) = fs::create_dir(&path) {
				if e.kind() != io::ErrorKind::AlreadyExists {
					Err(e)?;
				}
			}
			let list_url = format!("{}ilias.php?ref_id={}&cmdClass=xocteventgui&cmdNode=n7:mz:14p&baseClass=ilObjPluginDispatchGUI&lang=de&limit=20&cmd=asyncGetTableGUI&cmdMode=asynch", ILIAS_URL, url.ref_id);
			let data = ilias.download(&list_url);
			let html = data.await?.text().await?;
			let html = Html::parse_fragment(&html);
			for row in html.select(&video_tr) {
				let link = row.select(&a_target_blank).next();
				if link.is_none() {
					log!(0, "Warning: table row without link in {}", url.url);
					continue;
				}
				let link = link.unwrap();
				let mut cells = row.select(&td);
				if let Some(title) = cells.nth(2) {
					let title = title.inner_html();
					let title = title.trim();
					if title.starts_with("<div") {
						continue;
					}
					let mut path = path.clone();
					path.push(format!("{}.mp4", title));
					log!(1, "Found video: {}", title);
					let video = Video {
						url: URL::raw(link.value().attr("href").ok_or(anyhow!("video link without href"))?.to_owned())
					};
					let ilias = Arc::clone(&ilias);
					task::spawn(async {
						process_gracefully(ilias, path, video).await;
					});
				}
			}
		},
		Video { url } => {
			lazy_static!{
				static ref XOCT_REGEX: Regex = Regex::new(r#"(?m)<script>\s+xoctPaellaPlayer\.init\(([\s\S]+)\)\s+</script>"#).unwrap();
			}
			if ilias.opt.no_videos {
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
				let json = json.split(",\n").nth(0).context("invalid xoct player json")?;
				serde_json::from_str(&json.trim())?
			};
			log!(2, "{}", json);
			let url = json
				.pointer("/streams/0/sources/mp4/0/src")
				.map(|x| x.as_str())
				.ok_or(anyhow!("video src not found"))?
				.ok_or(anyhow!("video src not string"))?;
			if let Ok(meta) = fs::metadata(&path) {
				let head = ilias.client.head(url).send().await.context("HEAD request failed")?;
				if let Some(len) = head.headers().get("content-length") {
					if meta.len() != len.to_str()?.parse::<u64>()? {
						log!(0, "Warning: {} was updated, consider moving the outdated file", relative_path.to_string_lossy());
					}
				}
				log!(2, "Skipping download, file exists already");
				if !ilias.opt.force {
					return Ok(());
				}
			}
			let resp = ilias.download(&url).await?;
			let mut reader = stream_reader(resp.bytes_stream().map_err(|x| {
				io::Error::new(io::ErrorKind::Other, x)
			}));
			log!(0, "Writing {}", relative_path.to_string_lossy());
			let file = AsyncFile::create(&path).await?;
			let mut file = BufWriter::new(file);
			tokio::io::copy(&mut reader, &mut file).await?;
		},
		Forum { url, .. } => {
			if !ilias.opt.forum {
				return Ok(());
			}
			if let Err(e) = fs::create_dir(&path) {
				if e.kind() != io::ErrorKind::AlreadyExists {
					Err(e)?;
				}
			}
			let url = format!("{}ilias.php?ref_id={}&cmd=showThreads&cmdClass=ilrepositorygui&cmdNode=uf&baseClass=ilrepositorygui", ILIAS_URL, url.ref_id);
			let html = {
				let data = ilias.download(&url);
				let html_text = data.await?.text().await?;
				let url = {
					let html = Html::parse_document(&html_text);
					//https://ilias.studium.kit.edu/ilias.php?ref_id=122&cmdClass=ilobjforumgui&frm_tt_e39_122_trows=800&cmd=showThreads&cmdNode=uf:lg&baseClass=ilrepositorygui
					html
						.select(&a)
						.flat_map(|x| x.value().attr("href"))
						.filter(|x| x.contains("trows=800"))
						.next()
						.context("can't find forum thread count selector (empty forum?)")?.to_owned()
				};
				let data = ilias.download(&url);
				let html = data.await?.text().await?;
				Html::parse_document(&html)
			};
			for row in html.select(&tr) {
				let cells = row.select(&td).collect::<Vec<_>>();
				if cells.len() != 6 {
					log!(0, "Warning: unusual table row ({} cells) in {}", cells.len(), url);
					continue;
				}
				let link = cells[1].select(&a).next().context("thread link not found")?;
				let object = Object::from_link(link, link)?;
				let mut path = path.clone();
				let name = format!("{}_{}",
					object.url().thr_pk.as_ref().context("thr_pk not found for thread")?,
					link.text().collect::<String>().replace('/', "-").trim()
				);
				path.push(name);
				// TODO: set modification date?
				let saved_posts = {
					match fs::read_dir(&path) {
						Ok(stream) => stream.count(),
						Err(_) => 0
					}
				};
				let available_posts = cells[3].text().next().unwrap_or_default().trim().parse::<usize>().context("parsing post count failed")?;
				if available_posts <= saved_posts && !ilias.opt.force {
					continue;
				}
				log!(0, "New posts in {:?}..", path);
				let ilias = Arc::clone(&ilias);
				task::spawn(async {
					process_gracefully(ilias, path, object).await;
				});
			}
			if html.select(&forum_pages).count() > 0 {
				log!(0, "Ignoring older threads in {:?}..", path);
			}
		},
		Thread { url } => {
			if !ilias.opt.forum {
				return Ok(());
			}
			if let Err(e) = fs::create_dir(&path) {
				if e.kind() != io::ErrorKind::AlreadyExists {
					Err(e)?;
				}
			}
			let html = ilias.get_html(&url.url).await?;
			for post in html.select(&post_row) {
				let title = post.select(&post_title).next().ok_or(anyhow!("post title not found"))?.text().collect::<String>().replace('/', "-");
				let author = post.select(&span_small).next().ok_or(anyhow!("post author not found"))?;
				let author = author.text().collect::<String>();
				let author = author.trim().split('|').nth(1).ok_or(anyhow!("author data in unknown format"))?.trim();
				let container = post.select(&post_container).next().ok_or(anyhow!("post container not found"))?;
				let link = container.select(&a).next().ok_or(anyhow!("post link not found"))?;
				let name = format!("{}_{}_{}.html", link.value().attr("name").ok_or(anyhow!("post name in link not found"))?, author, title.trim());
				let data = post.select(&post_content).next().ok_or(anyhow!("post content not found"))?;
				let data = data.inner_html();
				let mut path = path.clone();
				path.push(name);
				let ilias = Arc::clone(&ilias);
				task::spawn(async move {
					*TASKS_QUEUED.lock() += 1;
					while *TASKS_RUNNING.lock() >= ilias.opt.jobs {
						tokio::time::delay_for(Duration::from_millis(100)).await;
					}
					*TASKS_RUNNING.lock() += 1;
					log!(2, "Writing to {:?}..", path);
					let file = AsyncFile::create(&path).await;
					if file.is_err() {
						log!(0, "Error creating file {:?}: {}", path, file.err().unwrap());
						return;
					}
					let mut file = BufWriter::new(file.unwrap());
					if let Err(e) = tokio::io::copy(&mut data.as_bytes(), &mut file).await {
						log!(0, "Error writing to {:?}: {}", path, e);
					}
					*TASKS_RUNNING.lock() -= 1;
					*TASKS_QUEUED.lock() -= 1;
				});
			}
			// pagination
			if let Some(pages) = html.select(&table).next() {
				if let Some(last) = pages.select(&links_in_table).last() {
					let text = last.text().collect::<String>();
					if text.trim() == ">>" {
						// not last page yet
						let ilias = Arc::clone(&ilias);
						let next_page = Thread {
							url: URL::from_href(last.value().attr("href").ok_or(anyhow!("page link not found"))?)
						};
						task::spawn(async move {
							process_gracefully(ilias, path, next_page).await;
						});
					}
				} else {
					log!(0, "Warning: unable to find pagination links in {}", url.url);
				}
			}
		},
		ExerciseHandler { url, .. } => {
			if let Err(e) = fs::create_dir(&path) {
				if e.kind() != io::ErrorKind::AlreadyExists {
					Err(e)?;
				}
			}
			let html = ilias.get_html(&url.url).await?;
			for link in html.select(&a) {
				let href = link.value().attr("href");
				if href.is_none() {
					continue;
				}
				let href = href.unwrap();
				let url = URL::from_href(href);
				if url.cmd.as_deref().unwrap_or("") != "downloadFile" {
					continue;
				}
				// link is definitely just a download link to the exercise
				let name = url.file.clone().context("link without file name")?;
				let item = File { url, name };
				let mut path = path.clone();
				path.push(item.name());
				let ilias = Arc::clone(&ilias);
				task::spawn(async {
					process_gracefully(ilias, path, item).await;
				});
			}
		},
		Weblink { url, .. } => {
			if !ilias.opt.force && fs::metadata(&path).is_ok() {
				log!(2, "Skipping download, link exists already");
				return Ok(());
			}
			let head = ilias.client.head(&url.url).send().await.context("HEAD request failed")?;
			let url = head.url().as_str();
			if url.starts_with(ILIAS_URL) {
				// is a link list
				if let Err(e) = fs::create_dir(&path) {
					if e.kind() != io::ErrorKind::AlreadyExists {
						Err(e)?;
					}
				} else {
					log!(0, "Writing {}", relative_path.to_string_lossy());
				}

				let urls = {
					let html = ilias.get_html(url).await?;
					html.select(&a)
						.filter_map(|x| x.value().attr("href").map(|y| (y, x.text().collect::<String>())))
						.map(|(x, y)| (URL::from_href(x), y.trim().to_owned()))
						.collect::<Vec<_>>()
				};

				for (url, name) in urls {
					if url.cmd.as_deref().unwrap_or("") != "callLink" {
						continue;
					}

					let head = ilias.client.head(url.url.as_str()).send().await.context("HEAD request to web link failed");
					if head.is_err() {
						println!("Warning: {:?}", head.err().unwrap());
						continue;
					}
					let head = head.unwrap();
					let url = head.url().as_str();
					path.push(name);
					let file = AsyncFile::create(&path).await?;
					let mut file = BufWriter::new(file);
					tokio::io::copy(&mut url.as_bytes(), &mut file).await?;
					path.pop();
				}
			} else {
				log!(0, "Writing {}", relative_path.to_string_lossy());
				let file = AsyncFile::create(&path).await?;
				let mut file = BufWriter::new(file);
				tokio::io::copy(&mut url.as_bytes(), &mut file).await?;
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
		}
	}
	Ok(())
}}

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

	/// Verbose logging (print objects downloaded)
	#[structopt(short, multiple = true, parse(from_occurrences))]
	verbose: usize,

	/// Output directory
	#[structopt(short, long, parse(from_os_str))]
	output: PathBuf,

	/// Parallel download jobs
	#[structopt(short, long, default_value = "1")]
	jobs: usize,
}

struct ILIAS {
	opt: Opt,
	ignore: Gitignore,
	// TODO: use these for re-authentication in case of session timeout/invalidation
	user: String,
	pass: String,
	client: Client
}

impl ILIAS {
	async fn login<S1: Into<String>, S2: Into<String>>(mut opt: Opt, user: S1, pass: S2) -> Result<Self> {
		let user = user.into();
		let pass = pass.into();
		let client = Client::builder()
			.cookie_store(true)
			.user_agent(concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION")))
			// timeout is infinite by default
			.build()?;
		// load .iliasignore file
		opt.output.push(".iliasignore");
		let (ignore, error) = Gitignore::new(&opt.output);
		if let Some(err) = error {
			println!("Warning: .iliasignore error: {}", err);
		}
		opt.output.pop();
		let this = ILIAS {
			opt, client, user, pass, ignore
		};
		println!("Logging into ILIAS using KIT account..");
		let session_establishment = this.client
			.post("https://ilias.studium.kit.edu/Shibboleth.sso/Login")
			.form(&json!({
				"sendLogin": "1",
				"idp_selection": "https://idp.scc.kit.edu/idp/shibboleth",
				"target": "https://ilias.studium.kit.edu/shib_login.php?target=",
				"home_organization_selection": "Mit KIT-Account anmelden"
			}))
			.send().await?;
		println!("Logging into Shibboleth..");
		let login_response = this.client
			.post(session_establishment.url().clone())
			.form(&json!({
				"j_username": &this.user,
				"j_password": &this.pass,
				"_eventId_proceed": ""
			}))
			.send().await?.text().await?;
		let dom = Html::parse_document(&login_response);
		/* TODO: OTP
		login_soup = BeautifulSoup(login_response.text, 'lxml')
		otp_inp = login_soup.find("input", attrs={"name": "j_tokenNumber"})
		if otp_inp:
			print("OTP Detected.")
			otp = input("OTP token: ")
			otp_url = otp_inp.parent.parent.parent['action']
			otp_response = self.post('https://idp.scc.kit.edu'+otp_url, data={'j_tokenNumber':otp, "_eventId_proceed": ""})
			login_soup = BeautifulSoup(otp_response.text, 'lxml')
		*/
		let saml = Selector::parse(r#"input[name="SAMLResponse"]"#).unwrap();
		let saml = dom.select(&saml).next().context("no SAML response, incorrect password?")?;
		let relay_state = Selector::parse(r#"input[name="RelayState"]"#).unwrap();
		let relay_state = dom.select(&relay_state).next().context("no relay state")?;
		println!("Logging into ILIAS..");
		this.client
			.post("https://ilias.studium.kit.edu/Shibboleth.sso/SAML2/POST")
			.form(&json!({
				"SAMLResponse": saml.value().attr("value").ok_or(anyhow!("no SAML value"))?,
				"RelayState": relay_state.value().attr("value").ok_or(anyhow!("no RelayState value"))?
			}))
			.send().await?;
		println!("Logged in!");
		Ok(this)
	}

	async fn download(&self, url: &str) -> Result<reqwest::Response> {
		if self.opt.verbose > 1 {
			println!("Downloading {}", url);
		}
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
		html.select(&container_items).map(|item| {
			item
				.select(&container_item_title)
				.next()
				.map(|link| Object::from_link(item, link))
				.unwrap_or_else(|| Err(anyhow!("can't find link")))
		}).collect()
	}

	async fn get_course_content(&self, url: &URL) -> Result<Vec<Result<Object>>> {
		let html = self.get_html(&url.url).await?;
		Ok(ILIAS::get_items(&html))
	}

	async fn personal_desktop(&self) -> Result<Dashboard> {
		let html = self.get_html("https://ilias.studium.kit.edu/ilias.php?baseClass=ilPersonalDesktopGUI&cmd=jumpToSelectedItems").await?;
		let items = ILIAS::get_items(&html).into_iter().flat_map(Result::ok).collect();
		Ok(Dashboard {
			items
		})
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
			let href = link.value().attr("href").unwrap_or("");
			if href == "" {
				// disabled course
				continue;
			}
			items.push(Object::from_link(link, link)?);
		}
		Ok(items)
	}
}

#[derive(Debug)]
struct Dashboard {
	items: Vec<Object>
}

#[derive(Debug)]
enum Object {
	Course {
		name: String,
		url: URL
	},
	Folder {
		name: String,
		url: URL
	},
	File {
		name: String,
		url: URL
	},
	Forum {
		name: String,
		url: URL
	},
	Thread {
		url: URL
	},
	Wiki {
		name: String,
		url: URL
	},
	ExerciseHandler {
		name: String,
		url: URL
	},
	Weblink {
		name: String,
		url: URL
	},
	Survey {
		name: String,
		url: URL
	},
	Presentation {
		name: String,
		url: URL
	},
	PluginDispatch {
		name: String,
		url: URL
	},
	Video {
		url: URL,
	},
	Generic {
		name: String,
		url: URL
	},
}

use Object::*;

impl Object {
	fn name(&self) -> &str {
		match self {
			Course { name, .. } => &name,
			Folder { name, .. } => &name,
			File { name, .. } => &name,
			Forum { name, .. } => &name,
			Thread { url } => &url.thr_pk.as_ref().unwrap(),
			Wiki { name, .. } => &name,
			Weblink { name, ..} => &name,
			Survey { name, .. } => &name,
			Presentation { name, .. } => &name,
			ExerciseHandler { name, .. } => &name,
			PluginDispatch { name, .. } => &name,
			Video { url } => &url.url,
			Generic { name, .. } => &name,
		}
	}

	fn url(&self) -> &URL {
		match self {
			Course { url, .. } => &url,
			Folder { url, .. } => &url,
			File { url, .. } => &url,
			Forum { url, .. } => &url,
			Thread { url } => &url,
			Wiki { url, .. } => &url,
			Weblink { url, .. } => &url,
			Survey { url, .. } => &url,
			Presentation { url, .. } => &url,
			ExerciseHandler { url, .. } => &url,
			PluginDispatch { url, .. } => &url,
			Video { url } => &url,
			Generic { url, .. } => &url,
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
			Course { .. } |
				Folder { .. } |
				Forum { .. } |
				Thread { .. } |
				Wiki { .. } |
				ExerciseHandler { .. } |
				Presentation { .. } |
				PluginDispatch { .. } => true,
			File { .. } |
				Video { .. } |
				Weblink { .. } |
				Survey { .. } |
				Generic { .. } => false
		}
	}

	fn from_link(item: ElementRef, link: ElementRef) -> Result<Self> {
		let mut name = link.text().collect::<String>().replace('/', "-").trim().to_owned();
		let mut url = URL::from_href(link.value().attr("href").context("link missing href")?);

		if url.thr_pk.is_some() {
			return Ok(Thread {
				url
			});
		}

		if url.url.starts_with("https://ilias.studium.kit.edu/goto.php") {
			if url.target.as_ref().map(|x| x.starts_with("wiki_")).unwrap_or(false) {
				return Ok(Wiki {
					name,
					url // TODO: insert ref_id here
				});
			}
			if url.target.as_ref().map(|x| x.starts_with("root_")).unwrap_or(false) {
				// magazine link
				return Ok(Generic {
					name,
					url
				});
			}
			if url.target.as_ref().map(|x| x.starts_with("crs_")).unwrap_or(false) {
				let ref_id = url.target.as_ref().unwrap().split('_').nth(1).unwrap();
				url.ref_id = ref_id.to_owned();
				return Ok(Course {
					name,
					url
				});
			}
			if url.target.as_ref().map(|x| x.starts_with("frm_")).unwrap_or(false) {
				// TODO: extract post link? (this codepath should only be hit when parsing the content tree)
				let ref_id = url.target.as_ref().unwrap().split('_').nth(1).unwrap();
				url.ref_id = ref_id.to_owned();
				return Ok(Forum {
					name,
					url
				});
			}
			if url.target.as_ref().map(|x| x.starts_with("lm_")).unwrap_or(false) {
				// fancy interactive task
				return Ok(Generic {
					name,
					url
				});
			}
			if url.target.as_ref().map(|x| x.starts_with("fold_")).unwrap_or(false) {
				let ref_id = url.target.as_ref().unwrap().split('_').nth(1).unwrap();
				url.ref_id = ref_id.to_owned();
				return Ok(Folder {
					name,
					url
				});
			}
			if url.target.as_ref().map(|x| x.starts_with("file_")).unwrap_or(false) {
				let target = url.target.as_ref().ok_or(anyhow!("no download target"))?;
				if !target.ends_with("download") {
					// download page containing metadata
					return Ok(Generic {
						name,
						url
					});
				} else {
					let item_prop = Selector::parse("span.il_ItemProperty").unwrap();
					let mut item_props = item.select(&item_prop);
					let ext = item_props.next().ok_or(anyhow!("cannot find file extension"))?;
					let version = item_props.nth(1).ok_or(anyhow!("cannot find 3rd file metadata"))?.text().collect::<String>();
					let version = version.trim();
					if version.starts_with("Version: ") {
						name.push_str("_v");
						name.push_str(&version[9..]);
					}
					return Ok(File { name: format!("{}.{}", name, ext.text().collect::<String>().trim()), url });
				}
			}
			return Ok(Generic { name, url });
		}

		if url.cmd.as_ref().map(|x| &**x) == Some("showThreads") {
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
				Some("view") => Folder { name, url },
				Some(_) => Generic { name, url },
				None => Course { name, url },
			},
			"ilobjplugindispatchgui" => PluginDispatch { name, url },
			_ => Generic { name, url }
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

	fn from_href(href: &str) -> Self {
		let url = if !href.starts_with(ILIAS_URL) {
			Url::parse(&format!("{}{}", ILIAS_URL, href)).unwrap()
		} else {
			Url::parse(href).unwrap()
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
				_ => {}
			}
		}
		URL {
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
		}
	}
}
