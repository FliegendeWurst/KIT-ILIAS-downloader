use error_chain::error_chain;

use super::*;

error_chain! {
	types {
		Error, ErrorKind, ResultExt, Result;
	}

	links {

	}

	foreign_links {
		Io(std::io::Error);
		Json(serde_json::Error);
		Reqwest(reqwest::Error);
	}

	errors {

	}

	//skip_msg_variant
}