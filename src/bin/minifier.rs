extern crate hyperbuild;
use hyperbuild::hyperbuild;
use std::{env, io, io::{Read, Write}};

fn main() {
	if env::args().nth(1) != Some("html".to_string()) {
		return
	}

	let mut stdin = Vec::new();
	io::stdin().lock().read_to_end(&mut stdin).expect("Unable to read from stdin!");

	let minified_len = hyperbuild(&mut stdin).expect("Unable to minify file!");
	stdin.truncate(minified_len);

	io::stdout().lock().write_all(&stdin).expect("Unable to write to stdout!");
}
