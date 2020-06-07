#![warn(clippy::nursery)]
#![warn(clippy::pedantic)]
#![warn(clippy::cargo)]
#![allow(clippy::cargo_common_metadata)]
#![allow(clippy::multiple_crate_versions)]
#![warn(clippy::all)]

use comrak::{Arena, parse_document, format_html, ComrakOptions};
use glob::glob;
use pulldown_cmark::{Parser, html};
use rayon::prelude::*;
use serde_derive::Deserialize;
use std::{thread, string::String, path::PathBuf, fs, fs::File, io::{Error, Read, Write, BufWriter}, process::{exit, Command, Stdio}};

#[derive(Deserialize)]
struct Config {
	plugins_list: Vec<String>,
	files: Files,
	markdown: Markdown,
}

#[derive(Deserialize)]
struct Files {
	input_glob: String,
	output_dir: PathBuf,
}

#[derive(Deserialize)]
struct Markdown {
	github_extensions: bool,
	comrak_extensions: bool,
}

const DEFAULT_CONFIG: &str = "plugins_list = []
[files]
input_glob = \"./*.md\"
output_dir = \".\"
[markdown]
github_extensions = false
comrak_extensions = false";

fn init_plugins(hook: String, list: Vec<String>) -> thread::JoinHandle<()> {
	thread::spawn(move || {
		list.par_iter().for_each(|plugin| {
			let mut child = Command::new(PathBuf::from("plugins/").join(plugin))
				.arg(&hook)
				.stdin(Stdio::null())
				.stdout(Stdio::inherit())
				.stderr(Stdio::inherit())
				.spawn().unwrap_or_else(|err| {
					eprintln!("Unable to start plugin {}! Additional info below:\n{}", plugin, err);
					exit(exitcode::OSERR);
				});
			let _ = child.wait();
		})
	})
}

fn run_plugins(mut buffer: &mut Vec<u8>, hook: &str, filename: &str, list: &[String]) {
	for plugin in list {
		let mut child = Command::new(PathBuf::from("plugins/").join(plugin))
			.arg(hook)
			.arg(filename)
			.stdin(Stdio::piped())
			.stdout(Stdio::piped())
			.stderr(Stdio::inherit())
			.spawn().unwrap_or_else(|err| {
				eprintln!("Unable to start plugin {}! Additional info below:\n{}", plugin, err);
				exit(exitcode::UNAVAILABLE);
			});

		child.stdin.as_mut().unwrap()
			.write_all(buffer).unwrap_or_else(|_| {
				eprintln!("Plugin {} crashed during usage!", plugin);
				exit(exitcode::UNAVAILABLE);
			});

		buffer.clear();
		drop(child.stdin.take());

		child.stdout.as_mut().unwrap()
			.read_to_end(&mut buffer).unwrap_or_else(|_| {
				eprintln!("Plugin {} crashed during usage!", plugin);
				exit(exitcode::UNAVAILABLE);
			});

		let _ = child.kill();
	}
}

fn markdown_to_html(input: &str, output: &mut dyn Write, github_ext: bool, comrak_ext: bool) -> Result<(), Error> {
	if github_ext || comrak_ext {
		let arena = &Arena::new();
		let options = &ComrakOptions {
			hardbreaks: false, // Don't let the user shoot themselves in the foot.
			smart: comrak_ext,
			github_pre_lang: true, // The lang tag makes a lot more sense for <code> blocks.
			width: 0, // Ignored when generating HTML
			default_info_string: None,
			unsafe_: true, // A proper HTML sanitizer should be used instead.
			ext_strikethrough: github_ext,
			ext_tagfilter: false, // A proper HTML sanitizer should be used instead.
			ext_table: github_ext,
			ext_autolink: github_ext,
			ext_tasklist: github_ext,
			ext_superscript: comrak_ext,
			ext_header_ids: {
				if github_ext {
					Some("".to_string())
				} else {
					None
				}
			},
			ext_footnotes: comrak_ext,
			ext_description_lists: comrak_ext,
		};
		let root = parse_document(arena, input, options);
		format_html(root, options, output)
	} else {
		let parser = Parser::new(input);
		html::write_html(output, parser)
	}
}

fn parse_to_file(input: &mut Vec<u8>, output: &mut dyn Write, filename: &str, config: &Config) -> Result<(), Error> {
	let mut output = BufWriter::new(output);
	if config.plugins_list.is_empty() {
		output.write_all(b"<!doctype html><meta name=viewport content=\"width=device-width,initial-scale=1\">")?;
	} else {
		run_plugins(input, "markdown", filename, &config.plugins_list);
	}
	markdown_to_html(
		&String::from_utf8_lossy(input),
		&mut output,
		config.markdown.github_extensions,
		config.markdown.comrak_extensions
	)?;

	output.flush()
}

fn main() {
	println!("Loading config...");
	let config_input = fs::read_to_string("conf.toml").unwrap_or_else(|_| {
		eprintln!("Warn: Unable to read config file!");
		DEFAULT_CONFIG.to_string()
	});
	let config: Config = toml::from_str(&config_input).unwrap_or_else(|err| {
		eprintln!("Unable to parse config file! Additional info below:\n{:#?}", err);
		exit(exitcode::CONFIG);
	});
	let files = glob(&config.files.input_glob).unwrap_or_else(|err| {
		eprintln!("Unable to parse file glob! Additional info below:\n{:#?}", err);
		exit(exitcode::CONFIG);
	}).par_bridge();

	if !config.files.output_dir.exists() {
		fs::create_dir_all(&config.files.output_dir).unwrap_or_else(|_| {
			eprintln!("Unable to create output directory!");
			exit(exitcode::CANTCREAT)
		});
	}

	let child = init_plugins("asyncinit".to_string(), config.plugins_list.to_owned());

	files.filter_map(Result::ok).for_each(|fpath| {
		let input_name = fpath.to_string_lossy();

		println!("Parsing {}...", input_name);
		let mut input = fs::read(&fpath).unwrap_or_else(|_| {
			eprintln!("Unable to open {:#?}!", &fpath);
			exit(exitcode::NOINPUT);
		});

		let output_path = config.files.output_dir.join(fpath.with_extension("html"));
		let mut output = File::create(&output_path).unwrap_or_else(|_| {
			eprintln!("Unable to create {:#?}!", &output_path);
			exit(exitcode::CANTCREAT);
		});

		parse_to_file(&mut input, &mut output, &input_name, &config).unwrap_or_else(|_| {
			eprintln!("Unable to finish parsing {:#?}!", &fpath);
			exit(exitcode::IOERR);
		});
	});

	let _ = init_plugins("postinit".to_string(), config.plugins_list).join();
	let _ = child.join();
}
