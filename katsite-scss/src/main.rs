#![warn(clippy::nursery)]
#![warn(clippy::pedantic)]
#![warn(clippy::cargo)]
#![allow(clippy::cargo_common_metadata)]
#![allow(clippy::multiple_crate_versions)]
#![warn(clippy::all)]

use sass_rs::{compile_file, OutputStyle::Expanded};
use serde_derive::Deserialize;
use std::{env, fs, io, io::{Read, Write}, process::{exit, Command, Stdio}, path::PathBuf};

#[derive(Deserialize)]
struct Config {
	files: Files,
	katsite_scss: Plugin,
}

#[derive(Deserialize)]
struct Files {
	output_dir: PathBuf,
}

#[derive(Deserialize)]
struct Plugin {
	stylesheet: PathBuf,
	minifier: bool,
}

fn main() {
	let command = env::args().nth(1);

	match command {
		Some(x) if x == "markdown" => {
			let mut stdin = Vec::new();
			io::stdin().lock().read_to_end(&mut stdin).unwrap();

			io::stdout().lock().write_all(&stdin).unwrap();
		},
		Some(x) if x == "asyncinit" => {
			let config_input = fs::read_to_string("conf.toml").unwrap_or_else(|_| {
				eprintln!("Unable to read config file!");
				exit(exitcode::NOINPUT)
			});
			let config: Config = toml::from_str(&config_input).unwrap_or_else(|err| {
				eprintln!("Unable to parse config file! Additional info below:\n{:#?}", err);
				exit(exitcode::CONFIG);
			});

			println!("Compiling {}...", config.katsite_scss.stylesheet.to_string_lossy());

			let output = compile_file(&config.katsite_scss.stylesheet, sass_rs::Options{
				output_style: Expanded,
				precision: 2,
				indented_syntax: false,
				include_paths: vec![],
			}).unwrap_or_else(|err| {
				eprintln!("Unable to parse {:#?}! Additional info below:\n{:#?}", &config.katsite_scss.stylesheet, err);
				exit(exitcode::DATAERR);
			});

			let output_file = config.files.output_dir.join("style.css");

			fs::write(&output_file, output).unwrap_or_else(|_| {
				eprintln!("Unable to write stylesheet!");
				exit(exitcode::IOERR);
			});

			if !config.katsite_scss.minifier {
				return
			}

			println!("Minifying {}...", config.katsite_scss.stylesheet.to_string_lossy());

			let mut child = Command::new("csso")
				.arg(&output_file)
				.arg("--output").arg(&output_file)
				.stdin(Stdio::null())
				.stdout(Stdio::inherit())
				.stderr(Stdio::inherit())
				.spawn().unwrap_or_else(|err| {
					eprintln!("Unable to start minifier! Additional info below:\n{}", err);
					exit(exitcode::UNAVAILABLE);
				});
			let _ = child.wait();
		},
		Some(x) if x == "postinit" => {
			exit(0);
		},
		_ => {
			eprintln!("KatSite Favicons is a plugin for KatSite, and is not meant to be used directly.");
			exit(exitcode::USAGE);
		},
	}
}
