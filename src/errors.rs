use error_chain::error_chain;

use super::*;

error_chain! {
	// The type defined for this error. These are the conventional
	// and recommended names, but they can be arbitrarily chosen.
	//
	// It is also possible to leave this section out entirely, or
	// leave it empty, and these names will be used automatically.
	types {
		Error, ErrorKind, ResultExt, Result;
	}

	// Without the `Result` wrapper:
	//
	// types {
	//     Error, ErrorKind, ResultExt;
	// }

	// Automatic conversions between this error chain and other
	// error chains. In this case, it will e.g. generate an
	// `ErrorKind` variant called `Another` which in turn contains
	// the `other_error::ErrorKind`, with conversions from
	// `other_error::Error`.
	//
	// Optionally, some attributes can be added to a variant.
	//
	// This section can be empty.
	links {
	//	Another(other_error::Error, other_error::ErrorKind) #[cfg(unix)];
	}

	// Automatic conversions between this error chain and other
	// error types not defined by the `error_chain!`. These will be
	// wrapped in a new error with, in the first case, the
	// `ErrorKind::Fmt` variant. The description and cause will
	// forward to the description and cause of the original error.
	//
	// Optionally, some attributes can be added to a variant.
	//
	// This section can be empty.
	foreign_links {
		//ALSA(alsa::Error);
		//Channel(crossbeam_channel::SendError); // TODO: requires type argument
		//Discord(serenity::Error);
		Io(std::io::Error);
		Reqwest(reqwest::Error);
		//Pulse(pulse::error::PAErr);
	}

	// Define additional `ErrorKind` variants.  Define custom responses with the
	// `description` and `display` calls.
	errors {
		UnsupportedChannel(x: String) {
			description("unsupported channel kind")
			display("unsupported channel kind {:?}", x)
		}
	}

	// If this annotation is left off, a variant `Msg(s: String)` will be added, and `From`
	// impls will be provided for `String` and `&str`
	//skip_msg_variant
}