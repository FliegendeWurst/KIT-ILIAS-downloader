// SPDX-License-Identifier: GPL-3.0-or-later

use std::{error::Error as _, io::Write, sync::Arc};

use anyhow::{anyhow, Context, Result};
use colored::Colorize;
use cookie_store::CookieStore;
use ignore::gitignore::Gitignore;
use reqwest::{Client, IntoUrl, Proxy, Url};
use reqwest_cookie_store::CookieStoreMutex;
use scraper::{ElementRef, Html, Selector};
use serde_json::json;

use crate::{cli::Opt, get_request_ticket, selectors::*, ILIAS_URL};

pub struct ILIAS {
	pub opt: Opt,
	pub ignore: Gitignore,
	client: Client,
	cookies: Arc<CookieStoreMutex>,
}

/// Returns true if the error is caused by:
/// "http2 error: protocol error: not a result of an error"
fn error_is_http2(error: &reqwest::Error) -> bool {
	error
		.source() // hyper::Error
		.map(|x| x.source()) // h2::Error
		.flatten()
		.map(|x| x.downcast_ref::<h2::Error>())
		.flatten()
		.map(|x| x.reason())
		.flatten()
		.map(|x| x == h2::Reason::NO_ERROR)
		.unwrap_or(false)
}

impl ILIAS {
	// TODO: de-duplicate the logic below
	pub async fn with_session(opt: Opt, session: Arc<CookieStoreMutex>, ignore: Gitignore) -> Result<Self> {
		let mut builder =
			Client::builder()
				.cookie_provider(Arc::clone(&session))
				.user_agent(concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION")));
		if let Some(proxy) = opt.proxy.as_ref() {
			let proxy = Proxy::all(proxy)?;
			builder = builder.proxy(proxy);
		}
		let client = builder
			// timeout is infinite by default
			.build()?;
		info!("Re-using previous session cookies..");
		Ok(ILIAS {
			opt,
			ignore,
			client,
			cookies: session,
		})
	}

	pub async fn login(opt: Opt, user: &str, pass: &str, ignore: Gitignore) -> Result<Self> {
		let cookie_store = CookieStore::default();
		let cookie_store = reqwest_cookie_store::CookieStoreMutex::new(cookie_store);
		let cookie_store = std::sync::Arc::new(cookie_store);
		let mut builder = Client::builder().cookie_provider(Arc::clone(&cookie_store)).user_agent(concat!(
			env!("CARGO_PKG_NAME"),
			"/",
			env!("CARGO_PKG_VERSION")
		));
		if let Some(proxy) = opt.proxy.as_ref() {
			let proxy = Proxy::all(proxy)?;
			builder = builder.proxy(proxy);
		}
		let client = builder
			// timeout is infinite by default
			.build()?;
		let this = ILIAS {
			opt,
			ignore,
			client,
			cookies: cookie_store,
		};
		info!("Logging into ILIAS using KIT account..");
		let session_establishment = this
			.client
			.post("https://ilias.studium.kit.edu/Shibboleth.sso/Login")
			.form(&json!({
				"sendLogin": "1",
				"idp_selection": "https://idp.scc.kit.edu/idp/shibboleth",
				"target": "/shib_login.php?target=",
				"home_organization_selection": "Mit KIT-Account anmelden"
			}))
			.send()
			.await?;
		let url = session_establishment.url().clone();
		let text = session_establishment.text().await?;
		let dom_sso = Html::parse_document(text.as_str());
		let csrf_token = dom_sso
			.select(&Selector::parse(r#"input[name="csrf_token"]"#).unwrap())
			.next()
			.context("no CSRF token found")?
			.value()
			.attr("value")
			.context("no CSRF token value")?;
		info!("Logging into Shibboleth..");
		let login_response = this
			.client
			.post(url)
			.form(&json!({
				"j_username": user,
				"j_password": pass,
				"_eventId_proceed": "",
				"csrf_token": csrf_token,
			}))
			.send()
			.await?
			.text()
			.await?;
		let dom = Html::parse_document(&login_response);
		let saml = Selector::parse(r#"input[name="SAMLResponse"]"#).unwrap();
		let saml = dom.select(&saml).next().context("no SAML response, incorrect password?")?;
		let relay_state = Selector::parse(r#"input[name="RelayState"]"#).unwrap();
		let relay_state = dom.select(&relay_state).next().context("no relay state")?;
		info!("Logging into ILIAS..");
		this.client
			.post("https://ilias.studium.kit.edu/Shibboleth.sso/SAML2/POST")
			.form(&json!({
				"SAMLResponse": saml.value().attr("value").context("no SAML value")?,
				"RelayState": relay_state.value().attr("value").context("no RelayState value")?
			}))
			.send()
			.await?;
		success!("Logged in!");
		Ok(this)
	}

	pub async fn save_session(&self) -> Result<()> {
		let session_path = self.opt.output.join(".iliassession");
		let mut writer = std::fs::File::create(session_path).map(std::io::BufWriter::new).unwrap();
		let store = self.cookies.lock().map_err(|x| anyhow!("{}", x))?;
		// save all cookies, including session cookies
		for cookie in store.iter_unexpired().map(serde_json::to_string) {
			writeln!(writer, "{}", cookie?)?;
		}
		writer.flush()?;
		Ok(())
	}

	pub async fn download(&self, url: &str) -> Result<reqwest::Response> {
		get_request_ticket().await;
		log!(2, "Downloading {}", url);
		let url = if url.starts_with("http://") || url.starts_with("https://") {
			url.to_owned()
		} else if url.starts_with("ilias.studium.kit.edu") {
			format!("https://{}", url)
		} else {
			format!("{}{}", ILIAS_URL, url)
		};
		for attempt in 1..10 {
			let result = self.client.get(url.clone()).send().await;
			match result {
				Ok(x) => return Ok(x),
				Err(e) if attempt <= 3 && error_is_http2(&e) => {
					warning!(1; "encountered HTTP/2 NO_ERROR, retrying download..");
					continue;
				},
				Err(e) => return Err(e.into()),
			}
		}
		unreachable!()
	}

	pub async fn head<U: IntoUrl>(&self, url: U) -> Result<reqwest::Response, reqwest::Error> {
		get_request_ticket().await;
		let url = url.into_url()?;
		for attempt in 1..10 {
			let result = self.client.head(url.clone()).send().await;
			match result {
				Ok(x) => return Ok(x),
				Err(e) if attempt <= 3 && error_is_http2(&e) => {
					warning!(1; "encountered HTTP/2 NO_ERROR, retrying HEAD request..");
					continue;
				},
				Err(e) => return Err(e),
			}
		}
		unreachable!()
	}

	pub async fn get_html(&self, url: &str) -> Result<Html> {
		let resp = self.download(url).await?;
		if resp
			.url()
			.query()
			.map(|x| x.contains("reloadpublic=1") || x.contains("cmd=force_login"))
			.unwrap_or(false)
		{
			return Err(anyhow!("not logged in / session expired"));
		}
		let text = self.download(url).await?.text().await?;
		let html = Html::parse_document(&text);
		if html.select(&alert_danger).next().is_some() {
			Err(anyhow!("ILIAS error"))
		} else {
			Ok(html)
		}
	}

	pub async fn get_html_fragment(&self, url: &str) -> Result<Html> {
		let text = self.download(url).await?.text().await?;
		let html = Html::parse_fragment(&text);
		if html.select(&alert_danger).next().is_some() {
			Err(anyhow!("ILIAS error"))
		} else {
			Ok(html)
		}
	}

	pub fn get_items(html: &Html) -> Vec<Result<Object>> {
		html.select(&container_items)
			.flat_map(|item| {
				item.select(&container_item_title).next().map(|link| Object::from_link(item, link))
				// items without links are ignored
			})
			.collect()
	}

	/// Returns subfolders and the main text in a course/folder/personal desktop.
	pub async fn get_course_content(&self, url: &URL) -> Result<(Vec<Result<Object>>, Option<String>)> {
		let html = self.get_html(&url.url).await?;

		let main_text = if let Some(el) = html.select(&il_content_container).next() {
			if !el
				.children()
				.flat_map(|x| x.value().as_element())
				.next()
				.map(|x| x.attr("class").unwrap_or_default().contains("ilContainerBlock"))
				.unwrap_or(false)
				&& el.inner_html().len() > 40
			{
				// ^ minimum length of useful content?
				Some(el.inner_html())
			} else {
				// first element is the content overview => no custom text (?)
				None
			}
		} else {
			None
		};
		Ok((ILIAS::get_items(&html), main_text))
	}

	pub async fn get_course_content_tree(&self, ref_id: &str, cmd_node: &str) -> Result<Vec<Object>> {
		// TODO: this magically does not return sub-folders
		// opening the same url in browser does show sub-folders?!
		let url = format!(
			"{}ilias.php?ref_id={}&cmdClass=ilobjcoursegui&cmd=showRepTree&cmdNode={}&baseClass=ilRepositoryGUI&cmdMode=asynch&exp_cmd=getNodeAsync&node_id=exp_node_rep_exp_{}&exp_cont=il_expl2_jstree_cont_rep_exp&searchterm=",
			ILIAS_URL, ref_id, cmd_node, ref_id
		);
		let html = self.get_html_fragment(&url).await?;
		let mut items = Vec::new();
		for link in html.select(&LINKS) {
			if link.value().attr("href").is_some() {
				items.push(Object::from_link(link, link)?);
			} // else: disabled course
		}
		Ok(items)
	}
}

#[derive(Debug)]
pub enum Object {
	Course { name: String, url: URL },
	Folder { name: String, url: URL },
	PersonalDesktop { url: URL },
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
	pub fn name(&self) -> &str {
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
			PersonalDesktop { .. } => panic!("name of personal desktop requested (this should never happen)"),
		}
	}

	pub fn url(&self) -> &URL {
		match self {
			Course { url, .. }
			| Folder { url, .. }
			| PersonalDesktop { url }
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

	pub fn kind(&self) -> &str {
		match self {
			Course { .. } => "course",
			Folder { .. } => "folder",
			PersonalDesktop { .. } => "personal desktop",
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

	pub fn is_dir(&self) -> bool {
		matches!(
			self,
			Course { .. }
				| Folder { .. } | PersonalDesktop { .. }
				| Forum { .. } | Thread { .. }
				| Wiki { .. } | ExerciseHandler { .. }
				| PluginDispatch { .. }
		)
	}

	pub fn from_link(item: ElementRef, link: ElementRef) -> Result<Self> {
		let name = link.text().collect::<String>().replace('/', "-").trim().to_owned();
		let url = URL::from_href(link.value().attr("href").context("link missing href")?)?;
		Object::from_url(url, name, Some(item))
	}

	pub fn from_url(mut url: URL, mut name: String, item: Option<ElementRef>) -> Result<Self> {
		if url.thr_pk.is_some() {
			return Ok(Thread { url });
		}

		if url.url.starts_with("https://ilias.studium.kit.edu/goto.php") {
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
					let mut item_props = item.context("can't construct file object without HTML object")?.select(&item_prop);
					let ext = item_props.next().context("cannot find file extension")?;
					let version = item_props.nth(1).context("cannot find 3rd file metadata")?.text().collect::<String>();
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
			"ilpersonaldesktopgui" => PersonalDesktop { url },
			_ => Generic { name, url },
		})
	}
}

#[allow(non_snake_case)]
#[derive(Debug)]
pub struct URL {
	pub url: String,
	baseClass: String,
	cmdClass: Option<String>,
	cmdNode: Option<String>,
	pub cmd: Option<String>,
	forwardCmd: Option<String>,
	pub thr_pk: Option<String>,
	pos_pk: Option<String>,
	pub ref_id: String,
	target: Option<String>,
	file: Option<String>,
}

#[allow(non_snake_case)]
impl URL {
	pub fn raw(url: String) -> Self {
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

	pub fn from_href(href: &str) -> Result<Self> {
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
			url: url.into(),
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
