use futures_util::stream::TryStreamExt;
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

use std::collections::VecDeque;
use std::default::Default;
use std::fs;
use std::io;
use std::panic;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

mod errors;
use errors::*;

const ILIAS_URL: &'static str = "https://ilias.studium.kit.edu/";

struct ILIAS {
	opt: Opt,
	// TODO: use these for re-authentication in case of session timeout/invalidation
	user: String,
	pass: String,
	client: Client
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
	PluginDispatch {
		name: String,
		url: URL
	},
	Video {
		url: String,
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
			ExerciseHandler { name, .. } => &name,
			PluginDispatch { name, .. } => &name,
			Video { url } => &url,
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
			ExerciseHandler { url, .. } => &url,
			PluginDispatch { url, .. } => &url,
			Video { .. } => unreachable!(),
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
			ExerciseHandler { .. } => "exercise handler",
			PluginDispatch { .. } => "plugin dispatch",
			Video { .. } => "video",
			Generic { .. } => "generic",
		}
	}

	fn from_link(item: ElementRef, link: ElementRef) -> Self {
		let mut name = link.text().collect::<String>().replace('/', "-");
		let url = URL::from_href(link.value().attr("href").unwrap());

		if url.thr_pk.is_some() {
			return Thread {
				url
			};
		}

		if url.url.starts_with("https://ilias.studium.kit.edu/goto.php") {
			let item_prop = Selector::parse("span.il_ItemProperty").unwrap();
			let mut item_props = item.select(&item_prop);
			let ext = item_props.next().unwrap();
			let version = item_props.nth(1).unwrap().text().collect::<String>();
			let version = version.trim();
			if version.starts_with("Version: ") {
				name.push_str("_v");
				name.push_str(&version[9..]);
			}
			return File { name: format!("{}.{}", name, ext.text().collect::<String>().trim()), url };
		}

		if url.cmd.as_ref().map(|x| &**x) == Some("showThreads") {
			return Forum { name, url };
		}

		match &*url.baseClass {
			"ilExerciseHandlerGUI" => ExerciseHandler { name, url },
			"ililWikiHandlerGUI" => Wiki { name, url },
			"ilrepositorygui" => match url.cmd.as_deref() {
				Some("view") => Folder { name, url },
				Some(_) => Generic { name, url },
				None => Course { name, url },
			},
			"ilObjPluginDispatchGUI" => PluginDispatch { name, url },
			_ => Generic { name, url }
		}
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
}

#[allow(non_snake_case)]
impl URL {
	fn from_href(href: &str) -> Self {
		let url = Url::parse(&format!("http://domain/{}", href)).unwrap();
		let mut baseClass = String::new();
		let mut cmdClass = None;
		let mut cmdNode = None;
		let mut cmd = None;
		let mut forwardCmd = None;
		let mut thr_pk = None;
		let mut pos_pk = None;
		let mut ref_id = String::new();
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
				_ => {}
			}
		}
		URL {
			url: href.to_owned(),
			baseClass,
			cmdClass,
			cmdNode,
			cmd,
			forwardCmd,
			thr_pk,
			pos_pk,
			ref_id
		}
	}
}

impl ILIAS {
	async fn login<S1: Into<String>, S2: Into<String>>(opt: Opt, user: S1, pass: S2) -> Result<Self> {
		let user = user.into();
		let pass = pass.into();
		let client = Client::builder()
			.cookie_store(true)
			.user_agent("KIT-ILIAS-downloader/0.2.0")
			.build()?;
		let this = ILIAS {
			opt, client, user, pass
		};
		println!("Logging into Shibboleth..");
		let session_establishment = this.client
			.post("https://ilias.studium.kit.edu/Shibboleth.sso/Login")
			.form(&json!({
				"sendLogin": "1",
				"idp_selection": "https://idp.scc.kit.edu/idp/shibboleth",
				"target": "https://ilias.studium.kit.edu/shib_login.php?target=",
				"home_organization_selection": "Mit KIT-Account anmelden"
			}))
			.send().await?;
		println!("Logging into identity provider..");
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
		let saml = dom.select(&saml).next().expect("no SAML response, incorrect password?");
		let relay_state = Selector::parse(r#"input[name="RelayState"]"#).unwrap();
		let relay_state = dom.select(&relay_state).next().expect("no relay state");
		println!("Logging into ILIAS..");
		this.client
			.post("https://ilias.studium.kit.edu/Shibboleth.sso/SAML2/POST")
			.form(&json!({
				"SAMLResponse": saml.value().attr("value").unwrap(),
				"RelayState": relay_state.value().attr("value").unwrap()
			}))
			.send().await?;
		println!("Logged in!");
		Ok(this)
	}

	async fn personal_desktop(&mut self) -> Result<Dashboard> {
		let html = self.get_html("https://ilias.studium.kit.edu/ilias.php?baseClass=ilPersonalDesktopGUI&cmd=jumpToSelectedItems").await?;
		let items = ILIAS::get_items(&html);
		Ok(Dashboard {
			items
		})
	}

	fn get_items(html: &Html) -> Vec<Object> {
		let container_items = Selector::parse("div.il_ContainerListItem").unwrap();
		let container_item_title = Selector::parse("a.il_ContainerItemTitle").unwrap();
		html.select(&container_items).map(|item| {
			let link = item.select(&container_item_title).next().unwrap();
			Object::from_link(item, link)
		}).collect()
	}

	async fn get_html(&self, url: &str) -> Result<Html> {
		let text = self.client.get(url).send().await?.text().await?;
		Ok(Html::parse_document(&text))
	}

	async fn get_course_content(&self, url: &URL) -> Result<Vec<Object>> {
		let html = self.get_html(&format!("{}{}", ILIAS_URL, url.url)).await?;
		Ok(ILIAS::get_items(&html))
	}

	async fn download(&self, url: &str) -> Result<reqwest::Response> {
		if self.opt.verbose > 0 {
			println!("Downloading {}", url);
		}
		Ok(self.client.get(url).send().await?)
	}
}

#[tokio::main]
async fn main() {
	let opt = Opt::from_args();
	*PANIC_HOOK.lock() = panic::take_hook();
	panic::set_hook(Box::new(|info| {
		*TASKS_RUNNING.lock() -= 1;
		PANIC_HOOK.lock()(info);
	}));
	let user = rprompt::prompt_reply_stdout("Username: ").unwrap();
	let pass = rpassword::read_password_from_tty(Some("Password: ")).unwrap();
	let mut ilias = match ILIAS::login::<_, String>(opt, user, pass).await {
		Ok(ilias) => ilias,
		Err(e) => panic!("error: {:?}", e)
	};
	let desktop = ilias.personal_desktop().await.unwrap();
	let mut queue = VecDeque::new();
	for item in desktop.items {
		let mut path = ilias.opt.output.clone();
		path.push(item.name());
		queue.push_back((path, item));
	}
	let ilias = Arc::new(ilias);
	while let Some((path, obj)) = queue.pop_front() {
		let ilias = Arc::clone(&ilias);
		task::spawn(async {
			while *TASKS_RUNNING.lock() > ilias.opt.jobs {
				tokio::time::delay_for(Duration::from_millis(100)).await;
			}
			*TASKS_RUNNING.lock() += 1;
			process(ilias, path, obj).await;
			*TASKS_RUNNING.lock() -= 1;
		});
	}
	while *TASKS_RUNNING.lock() > 0 {
		tokio::time::delay_for(Duration::from_millis(500)).await;
	}
}

lazy_static!{
	static ref TASKS_RUNNING: Mutex<usize> = Mutex::default();

	static ref PANIC_HOOK: Mutex<Box<dyn Fn(&panic::PanicInfo) + Sync + Send + 'static>> = Mutex::new(Box::new(|_| {}));
}

// see https://github.com/rust-lang/rust/issues/53690#issuecomment-418911229
//async fn process(ilias: Arc<ILIAS>, path: PathBuf, obj: Object) {
fn process(ilias: Arc<ILIAS>, path: PathBuf, obj: Object) -> impl std::future::Future<Output = ()> + Send { async move {
	if ilias.opt.verbose > 0 {
		println!("Syncing {} {}..", obj.kind(), path.strip_prefix(&ilias.opt.output).unwrap().to_string_lossy());
	}
	match &obj {
		Course { url, .. } => {
			if let Err(e) = fs::create_dir(&path) {
				if e.kind() != io::ErrorKind::AlreadyExists {
					println!("error: {:?}", e);
				}
			}
			let content = ilias.get_course_content(&url).await.unwrap();
			for item in content {
				let mut path = path.clone();
				path.push(item.name());
				let ilias = Arc::clone(&ilias);
				task::spawn(async {
					while *TASKS_RUNNING.lock() > ilias.opt.jobs {
						tokio::time::delay_for(Duration::from_millis(100)).await;
					}
					*TASKS_RUNNING.lock() += 1;
					process(ilias, path, item).await;
					*TASKS_RUNNING.lock() -= 1;
				});
			}
		},
		Folder { url, .. } => {
			if let Err(e) = fs::create_dir(&path) {
				if e.kind() != io::ErrorKind::AlreadyExists {
					println!("error: {:?}", e);
				}
			}
			let content = ilias.get_course_content(&url).await.unwrap();
			for item in content {
				let mut path = path.clone();
				path.push(item.name());
				let ilias = Arc::clone(&ilias);
				task::spawn(async {
					while *TASKS_RUNNING.lock() > ilias.opt.jobs {
						tokio::time::delay_for(Duration::from_millis(100)).await;
					}
					*TASKS_RUNNING.lock() += 1;
					process(ilias, path, item).await;
					*TASKS_RUNNING.lock() -= 1;
				});
			}
		},
		File { url, .. } => {
			if ilias.opt.skip_files {
				return;
			}
			if !ilias.opt.force && fs::metadata(&path).is_ok() {
				if ilias.opt.verbose > 1 {
					println!("Skipping download, file exists already");
				}
				return;
			}
			let data = ilias.download(&url.url).await;
			match data {
				Ok(resp) => {
					let mut reader = stream_reader(resp.bytes_stream().map_err(|x| {
						io::Error::new(io::ErrorKind::Other, x)
					}));
					let file = AsyncFile::create(&path).await.unwrap();
					let mut file = BufWriter::new(file);
					tokio::io::copy(&mut reader, &mut file).await.unwrap();
				},
				Err(e) => println!("error: {:?}", e)
			}
		},
		PluginDispatch { url, .. } => {
			if ilias.opt.no_videos {
				return;
			}
			if let Err(e) = fs::create_dir(&path) {
				if e.kind() != io::ErrorKind::AlreadyExists {
					println!("error: {:?}", e);
				}
			}
			let list_url = format!("{}ilias.php?ref_id={}&cmdClass=xocteventgui&cmdNode=n7:mz:14p&baseClass=ilObjPluginDispatchGUI&lang=de&limit=20&cmd=asyncGetTableGUI&cmdMode=asynch", ILIAS_URL, url.ref_id);
			let data = ilias.download(&list_url);
			let html = data.await.unwrap().text().await.unwrap();
			let html = Html::parse_fragment(&html);
			let tr = Selector::parse("tr").unwrap();
			let td = Selector::parse("td").unwrap();
			let a = Selector::parse(r#"a[target="_blank"]"#).unwrap();
			for row in html.select(&tr) {
				let link = row.select(&a).next();
				if link.is_none() {
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
					if ilias.opt.verbose > 0 {
						println!("Found video: {}", title);
					}
					let video = Video {
						url: link.value().attr("href").unwrap().to_owned()
					};
					let ilias = Arc::clone(&ilias);
					task::spawn(async {
						while *TASKS_RUNNING.lock() > ilias.opt.jobs {
							tokio::time::delay_for(Duration::from_millis(100)).await;
						}
						*TASKS_RUNNING.lock() += 1;
						process(ilias, path, video).await;
						*TASKS_RUNNING.lock() -= 1;
					});
				}
				
			}
		},
		Video { url } => {
			lazy_static!{
				static ref XOCT_REGEX: Regex = Regex::new(r#"(?m)<script>\s+xoctPaellaPlayer\.init\(([\s\S]+)\)\s+</script>"#).unwrap();
			}
			if ilias.opt.no_videos {
				return;
			}
			if !ilias.opt.force && fs::metadata(&path).is_ok() {
				if ilias.opt.verbose > 1 {
					println!("Skipping download, file exists already");
				}
				return;
			}
			let url = format!("{}{}", ILIAS_URL, url);
			let data = ilias.download(&url);
			let html = data.await.unwrap().text().await.unwrap();
			if ilias.opt.verbose > 1 {
				println!("{}", html);
			}
			let json: serde_json::Value = {
				let mut json_capture = XOCT_REGEX.captures_iter(&html);
				let json = &json_capture.next().unwrap()[1];
				if ilias.opt.verbose > 1 {
					println!("{}", json);
				}
				let json = json.split(",\n").nth(0).unwrap();
				serde_json::from_str(&json.trim()).unwrap()
			};
			if ilias.opt.verbose > 1 {
				println!("{}", json);
			}
			let url = json["streams"][0]["sources"]["mp4"][0]["src"].as_str().unwrap();
			let resp = ilias.download(&url).await.unwrap();
			let mut reader = stream_reader(resp.bytes_stream().map_err(|x| {
				io::Error::new(io::ErrorKind::Other, x)
			}));
			println!("Saving video to {:?}", path);
			let file = AsyncFile::create(&path).await.unwrap();
			let mut file = BufWriter::new(file);
			tokio::io::copy(&mut reader, &mut file).await.unwrap();
		},
		Forum { url, .. } => {
			if !ilias.opt.forum {
				return;
			}
			if let Err(e) = fs::create_dir(&path) {
				if e.kind() != io::ErrorKind::AlreadyExists {
					println!("error: {:?}", e);
				}
			}
			let url = format!("{}ilias.php?ref_id={}&cmd=showThreads&cmdClass=ilrepositorygui&cmdNode=uf&baseClass=ilrepositorygui", ILIAS_URL, url.ref_id);
			let html = {
				let a = Selector::parse("a").unwrap();
				let data = ilias.download(&url);
				let html_text = data.await.unwrap().text().await.unwrap();
				let url = {
					let html = Html::parse_document(&html_text);
					//https://ilias.studium.kit.edu/ilias.php?ref_id=122&cmdClass=ilobjforumgui&frm_tt_e39_122_trows=800&cmd=showThreads&cmdNode=uf:lg&baseClass=ilrepositorygui
					let url = {
						let t800 = html.select(&a).filter(|x| x.value().attr("href").unwrap_or("").contains("trows=800")).next().expect("can't find forum thread count selector");
						t800.value().attr("href").unwrap()
					};
					format!("{}{}", ILIAS_URL, url)
				};
				let data = ilias.download(&url);
				let html = data.await.unwrap().text().await.unwrap();
				Html::parse_document(&html)
			};
			let a = Selector::parse("a").unwrap();
			let tr = Selector::parse("tr").unwrap();
			let td = Selector::parse("td").unwrap();
			for row in html.select(&tr) {
				let cells = row.select(&td).collect::<Vec<_>>();
				if cells.len() != 6 {
					continue;
				}
				let link = cells[1].select(&a).next().unwrap();
				let object = Object::from_link(link, link);
				let mut path = path.clone();
				let name = format!("{}_{}", object.url().thr_pk.as_ref().expect("thr_pk not found for thread"), link.text().collect::<String>().replace('/', "-").trim());
				path.push(name);
				let ilias = Arc::clone(&ilias);
				task::spawn(async {
					while *TASKS_RUNNING.lock() > ilias.opt.jobs {
						tokio::time::delay_for(Duration::from_millis(100)).await;
					}
					*TASKS_RUNNING.lock() += 1;
					process(ilias, path, object).await;
					*TASKS_RUNNING.lock() -= 1;
				});
			}
		},
		Thread { url } => {
			if !ilias.opt.forum {
				return;
			}
			if let Err(e) = fs::create_dir(&path) {
				if e.kind() != io::ErrorKind::AlreadyExists {
					println!("error: {:?}", e);
				}
				// skip already downloaded
				// TODO: compare modification date
				if !ilias.opt.force {
					return;
				}
			}
			let url = format!("{}{}", ILIAS_URL, url.url);
			let data = ilias.download(&url);
			let html = data.await.unwrap().text().await.unwrap();
			let html = Html::parse_document(&html);
			let post = Selector::parse(".ilFrmPostRow").unwrap();
			let post_container = Selector::parse(".ilFrmPostContentContainer").unwrap();
			let post_title = Selector::parse(".ilFrmPostTitle").unwrap();
			let post_content = Selector::parse(".ilFrmPostContent").unwrap();
			let span_small = Selector::parse("span.small").unwrap();
			let a = Selector::parse("a").unwrap();
			for post in html.select(&post) {
				let title = post.select(&post_title).next().unwrap().text().collect::<String>().replace('/', "-");
				let author = post.select(&span_small).next().unwrap();
				let author = author.text().collect::<String>();
				let author = author.trim().split('|').nth(1).unwrap().trim();
				let container = post.select(&post_container).next().unwrap();
				let link = container.select(&a).next().unwrap();
				let name = format!("{}_{}_{}.html", link.value().attr("name").unwrap(), author, title.trim());
				let data = post.select(&post_content).next().unwrap();
				let data = data.inner_html();
				let mut path = path.clone();
				path.push(name);
				let ilias = Arc::clone(&ilias);
				task::spawn(async move {
					while *TASKS_RUNNING.lock() > ilias.opt.jobs {
						tokio::time::delay_for(Duration::from_millis(100)).await;
					}
					*TASKS_RUNNING.lock() += 1;
					if ilias.opt.verbose > 1 {
						println!("Writing to {:?}..", path);
					}
					let file = AsyncFile::create(&path).await.unwrap();
					let mut file = BufWriter::new(file);
					tokio::io::copy(&mut data.as_bytes(), &mut file).await.unwrap();
					*TASKS_RUNNING.lock() -= 1;
				});
			}
		},
		o => {
			if ilias.opt.verbose > 0 {
				println!("ignoring {:#?}", o)
			}
		}
	}
}}

#[derive(Debug, StructOpt)]
#[structopt(name = "KIT-ILIAS-downloader")]
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
