// SPDX-License-Identifier: GPL-3.0-or-later

use anyhow::{anyhow, Context, Result};
use futures::future::{self, Either};
use futures::StreamExt;
use ignore::gitignore::Gitignore;
use indicatif::{ProgressDrawTarget, ProgressStyle};
use structopt::StructOpt;
use tokio::fs;

use std::collections::HashMap;
use std::future::Future;
use std::io::BufReader;
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::SystemTime;

static ILIAS_URL: &str = "https://ilias.studium.kit.edu/";
/// main personal desktop
static DEFAULT_SYNC_URL: &str =
	"https://ilias.studium.kit.edu/ilias.php?baseClass=ilPersonalDesktopGUI&cmd=jumpToSelectedItems";

#[macro_use]
mod cli;
use cli::*;
mod ilias;
use ilias::*;
use Object::*;
mod queue;
mod util;
use util::*;

#[tokio::main]
async fn main() {
	let opt = Opt::from_args();
	if let Err(e) = real_main(opt).await {
		error!(e);
	}
}

async fn try_to_load_session(opt: Opt, ignore: Gitignore, course_names: HashMap<String, String>) -> Result<ILIAS> {
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
		Ok(ILIAS::with_session(opt, cookie_store, ignore, course_names).await?)
	} else {
		Err(anyhow!("session data too old"))
	}
}

async fn login(opt: Opt, ignore: Gitignore, course_names: HashMap<String, String>) -> Result<ILIAS> {
	// load .iliassession file
	if opt.keep_session {
		match try_to_load_session(opt.clone(), ignore.clone(), course_names.clone())
			.await
			.context("failed to load previous session")
		{
			Ok(ilias) => {
				info!("Checking session validity..");
				// TODO: this probably isn't the best solution..
				if let Err(e) = ilias.get_html(DEFAULT_SYNC_URL).await {
					error!(e)
				} else {
					success!("Session still active!");
					return Ok(ilias);
				}
			},
			Err(e) => warning!(e),
		}
	}

	// load .iliaslogin file
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

	let ilias = match ILIAS::login(opt, &user, &pass, ignore, course_names).await {
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

	create_dir(&opt.output)
		.await
		.context("failed to create output directory")?;
	// use UNC paths on Windows (to avoid the default max. path length of 255)
	opt.output = fs::canonicalize(opt.output)
		.await
		.context("failed to canonicalize output directory")?;

	// load .iliasignore file
	let (ignore, error) = Gitignore::new(opt.output.join(".iliasignore"));

	// Load course_names.toml file
	let course_names_path = opt.output.join("course_names.toml");
	let course_names: HashMap<String, String>;

	if fs::metadata(&course_names_path).await.is_ok() {
		fs::read_to_string(&course_names_path).await.context("reading course_names.toml")?;
		course_names = toml::from_str(&fs::read_to_string(course_names_path).await?).unwrap();
	} else {
		// If file doesn't exist, initialise course_names with empty HashMap
		course_names = HashMap::new();
	}
		
	if let Some(err) = error {
		warning!(err);
	}

	queue::set_download_rate(opt.rate);

	let ilias = login(opt, ignore, course_names).await?;

	if ilias.opt.content_tree {
		if let Err(e) = ilias
			.download("ilias.php?baseClass=ilRepositoryGUI&cmd=frameset&set_mode=tree&ref_id=1")
			.await
		{
			warning!("could not enable content tree:", e);
		}
	}
	let ilias = Arc::new(ilias);
	let mut rx = queue::set_parallel_jobs(ilias.opt.jobs);
	PROGRESS_BAR_ENABLED.store(atty::is(atty::Stream::Stdout), Ordering::SeqCst);
	if PROGRESS_BAR_ENABLED.load(Ordering::SeqCst) {
		PROGRESS_BAR.set_draw_target(ProgressDrawTarget::stderr_nohz());
		PROGRESS_BAR.set_style(ProgressStyle::default_bar().template("[{pos}/{len}+] {wide_msg}"));
		PROGRESS_BAR.set_message("initializing..");
	}

	let sync_url = ilias.opt.sync_url.as_deref().unwrap_or(DEFAULT_SYNC_URL);
	let obj = Object::from_url(
		URL::from_href(sync_url).context("invalid sync URL")?,
		String::new(),
		None,
	)
	.context("invalid sync object")?;
	queue::spawn(process_gracefully(ilias.clone(), ilias.opt.output.clone(), obj));

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
	if PROGRESS_BAR_ENABLED.load(Ordering::SeqCst) {
		PROGRESS_BAR.inc_length(1);
	}
	async move {
		let permit = queue::get_ticket().await;
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

async fn process(ilias: Arc<ILIAS>, path: PathBuf, obj: Object) -> Result<()> {
	let relative_path = path.strip_prefix(&ilias.opt.output).unwrap();
	if PROGRESS_BAR_ENABLED.load(Ordering::SeqCst) {
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
			ilias::course::download(path, ilias, url, name).await?;
		},
		Folder { url, .. } | PersonalDesktop { url } => {
			ilias::folder::download(&path, ilias, url).await?;
		},
		File { url, .. } => {
			ilias::file::download(&path, relative_path, ilias, url).await?;
		},
		PluginDispatch { url, .. } => {
			ilias::plugin_dispatch::download(&path, ilias, url).await?;
		},
		Video { url } => {
			ilias::video::download(&path, relative_path, ilias, url).await?;
		},
		Forum { url, .. } => {
			ilias::forum::download(&path, ilias, url).await?;
		},
		Thread { url } => {
			ilias::thread::download(&path, relative_path, ilias, url).await?;
		},
		ExerciseHandler { url, .. } => {
			ilias::exercise::download(&path, ilias, url).await?;
		},
		Weblink { url, .. } => {
			ilias::weblink::download(&path, relative_path, ilias, url).await?;
		},
		Wiki { .. } => {
			log!(1, "Ignored wiki!");
		},
		Survey { .. } => {
			log!(1, "Ignored survey!");
		},
		Presentation { .. } => {
			log!(
				1,
				"Ignored interactive presentation! (visit it yourself, it's probably interesting)"
			);
		},
		Generic { .. } => {
			log!(1, "Ignored generic {:?}", obj)
		},
	}
	if PROGRESS_BAR_ENABLED.load(Ordering::SeqCst) {
		PROGRESS_BAR.inc(1);
	}
	Ok(())
}
