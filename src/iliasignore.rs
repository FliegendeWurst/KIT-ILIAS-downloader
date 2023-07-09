use std::{
	ffi::OsString,
	path::{Component, Path, PathBuf},
};

use anyhow::Result;
use ignore::gitignore::Gitignore;

#[derive(Clone, Debug)]
pub struct IliasIgnore {
	ignores: Vec<IgnoreFile>,
}

impl IliasIgnore {
	pub fn load(mut path: PathBuf) -> Result<Self> {
		let mut ignores = Vec::new();
		let mut prefix = Vec::new();
		// example scenario:
		// path = /KIT/ILIAS/SS 23/Next Generation Internet
		// iliasignore in ILIAS/.iliasignore: prefix = SS 23/Next Generation Internet/
		// iliasignore in Next Generation Internet/.iliasignore: prefix = ""
		loop {
			let (ignore, error) = Gitignore::new(path.join(".iliasignore"));
			if let Some(err) = error {
				warning!(err);
			}
			if ignore.len() > 0 {
				ignores.push(IgnoreFile {
					ignore,
					prefix: prefix.iter().fold(OsString::new(), |mut acc, el| {
						acc.push(el);
						acc.push("/");
						acc
					}),
				});
			}
			if let Some(last) = path.components().last() {
				match last {
					Component::Normal(name) => prefix.insert(0, name.to_owned()),
					_ => break,
				}
			}
			path.pop();
		}
		Ok(IliasIgnore { ignores })
	}

	pub fn should_ignore(&self, path: &Path, is_dir: bool) -> bool {
		for ignore_file in &self.ignores {
			let mut full_path = ignore_file.prefix.clone();
			full_path.push(path.as_os_str());
			let matched = ignore_file.ignore.matched(&full_path, is_dir);
			if matched.is_whitelist() {
				return false;
			} else if matched.is_ignore() {
				return true;
			}
		}
		false
	}
}

#[derive(Clone, Debug)]
struct IgnoreFile {
	ignore: Gitignore,
	prefix: OsString,
}
