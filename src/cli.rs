// SPDX-License-Identifier: GPL-3.0-or-later

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize};

use anyhow::anyhow;
use anyhow::{Context, Result};
use indicatif::ProgressBar;
use once_cell::sync::Lazy;
use structopt::StructOpt;

#[derive(Debug, Clone, StructOpt)]
#[structopt(name = env!("CARGO_PKG_NAME"))]
pub struct Opt {
	/// Do not download files
	#[structopt(short, long)]
	pub skip_files: bool,

	/// Do not download Opencast videos
	#[structopt(short, long)]
	pub no_videos: bool,

	/// Download forum content
	#[structopt(short = "t", long)]
	pub forum: bool,

	/// Re-download already present files
	#[structopt(short)]
	pub force: bool,

	/// Use content tree (experimental)
	#[structopt(long)]
	pub content_tree: bool,

	/// Re-check OpenCast lectures (slow)
	#[structopt(long)]
	pub check_videos: bool,

	/// Combine videos if there is more than one stream (requires ffmpeg)
	#[structopt(long)]
	pub combine_videos: bool,

	/// Save overview pages of ILIAS courses and folders
	#[structopt(long)]
	pub save_ilias_pages: bool,

	/// Verbose logging
	#[structopt(short, multiple = true, parse(from_occurrences))]
	pub verbose: usize,

	/// Output directory
	#[structopt(short, long, parse(from_os_str))]
	pub output: PathBuf,

	/// Parallel download jobs
	#[structopt(short, long, default_value = "1")]
	pub jobs: usize,

	/// Proxy, e.g. socks5h://127.0.0.1:1080
	#[structopt(short, long)]
	pub proxy: Option<String>,

	/// Use the system keyring
	#[structopt(long)]
	pub keyring: bool,

	/// KIT account username
	#[structopt(short = "U", long)]
	pub username: Option<String>,

	/// KIT account password
	#[structopt(short = "P", long)]
	pub password: Option<String>,

	/// ILIAS page to download
	#[structopt(long)]
	pub sync_url: Option<String>,

	/// Requests per minute
	#[structopt(long, default_value = "8")]
	pub rate: usize,

	/// Attempt to re-use session cookies
	#[structopt(long)]
	pub keep_session: bool,
}

pub static LOG_LEVEL: AtomicUsize = AtomicUsize::new(0);
pub static PROGRESS_BAR_ENABLED: AtomicBool = AtomicBool::new(false);
pub static PROGRESS_BAR: Lazy<ProgressBar> = Lazy::new(|| ProgressBar::new(0));

macro_rules! log {
	($lvl:expr, $($t:expr),+) => {{
		#[allow(unused_imports)]
		use colored::Colorize as _;
		#[allow(unused_comparisons)] // 0 <= 0
		if $lvl <= crate::cli::LOG_LEVEL.load(std::sync::atomic::Ordering::SeqCst) {
			if crate::cli::PROGRESS_BAR_ENABLED.load(std::sync::atomic::Ordering::SeqCst) {
				crate::cli::PROGRESS_BAR.println(format!($($t),+));
			} else {
				println!($($t),+);
			}
		}
	}}
}

macro_rules! info {
	($t:tt) => {
		log!(0, $t);
	};
}

macro_rules! success {
	($t:tt) => {
		log!(0, "{}", format!($t).bright_green());
	};
}

macro_rules! warning {
	($e:expr) => {{
		log!(0, "Warning: {}", format!("{:?}", $e).bright_yellow());
	}};
	($msg:expr, $e:expr) => {{
		log!(0, "Warning: {}", format!("{} {:?}", $msg, $e).bright_yellow());
	}};
	($msg1:expr, $msg2:expr, $e:expr) => {{
		log!(0, "Warning: {}", format!("{} {} {:?}", $msg1, $msg2, $e).bright_yellow());
	}};
	(format => $($e:expr),+) => {{
		log!(0, "Warning: {}", format!($($e),+).bright_yellow());
	}};
	($lvl:expr; $($e:expr),+) => {{
		log!($lvl, "Warning: {}", format!($($e),+).bright_yellow());
	}};
}

macro_rules! error {
	($($prefix:expr),+; $e:expr) => {
		log!(0, "{}: {}", format!($($prefix),+), format!("{:?}", $e).bright_red());
	};
	($e:expr) => {
		log!(0, "Error: {}", format!("{:?}", $e).bright_red());
	};
}

pub fn ask_user_pass(opt: &Opt) -> Result<(String, String)> {
	let user = if let Some(username) = opt.username.as_ref() {
		username.clone()
	} else {
		rprompt::prompt_reply_stdout("Username: ").context("username prompt")?
	};
	let (pass, should_store);
	let keyring = Lazy::new(|| keyring::Entry::new(env!("CARGO_PKG_NAME"), &user));
	if let Some(password) = opt.password.as_ref() {
		pass = password.clone();
		should_store = true;
	} else if opt.keyring {
		match keyring.get_password() {
			Ok(password) => {
				pass = password;
				should_store = false;
			},
			Err(e) => {
				error!(e);
				pass = rpassword::read_password_from_tty(Some("Password: ")).context("password prompt")?;
				should_store = true;
			}
		}
	} else {
		pass = rpassword::read_password_from_tty(Some("Password: ")).context("password prompt")?;
		should_store = true;
	}
	if should_store && opt.keyring {
		keyring.set_password(&pass).map_err(|x| anyhow!(x.to_string()))?;
	}
	Ok((user, pass))
}
