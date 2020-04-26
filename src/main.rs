#![deny(clippy::nursery)]
#![deny(clippy::pedantic)]
#![allow(clippy::cargo)]
#![deny(clippy::all)]

extern crate comrak;
extern crate exitcode;
extern crate glob;
extern crate minifier;
extern crate rayon;
extern crate serde_derive;
extern crate toml;
use comrak::{markdown_to_html, ComrakOptions};
use glob::glob;
use rayon::prelude::*;
use serde_derive::Deserialize;
use std::{string::String, path::PathBuf, fs, fs::File, io::Write, process::{exit, Command, Stdio}};

#[derive(Deserialize)]
struct Config {
	markdown: Markdown,
	plugins: Plugins,
}

#[derive(Deserialize)]
struct Markdown {
	convert_line_breaks: bool,
	convert_punctuation: bool,
	enable_raw_html_inlining: bool,
	enable_github_extensions: bool,
	enable_comrak_extensions: bool,
}

#[derive(Deserialize)]
struct Plugins {
	enable_core: bool,
	plugins_list: Vec<String>,
}

fn run_plugins(mut input: std::vec::Vec<u8>, hook: &str, config: &Config) -> Result<std::vec::Vec<u8>, Box<dyn std::error::Error>> {
	for plugin in &config.plugins.plugins_list {
		let mut child = Command::new(PathBuf::from(plugin).canonicalize()?)
			.arg(hook)
			.stdin(Stdio::piped())
			.stdout(Stdio::piped())
			.spawn()?;

		if child.stdin.as_mut().unwrap().write_all(&input).is_err() {
			continue
		}

		let output = child.wait_with_output()?;
		if output.status.success() {
			if !output.stdout.is_empty() {
				input = output.stdout;
			}
		} else {
			println!("Warn: Plugin {} returned a non-zero exit code, discarding it's output...", plugin);
		}
	}
	Ok(input)
}

fn parse_to_html(mut markdown_input: String, config: &Config) -> Result<std::vec::Vec<u8>, Box<dyn std::error::Error>> {
	let plugin_output = run_plugins(markdown_input.as_bytes().to_vec(), "markdown", config)?;
	markdown_input = String::from_utf8_lossy(&plugin_output).to_string();

	let html_output = markdown_to_html(&markdown_input, &ComrakOptions {
		hardbreaks: config.markdown.convert_line_breaks,
		smart: config.markdown.convert_punctuation,
		github_pre_lang: true, // The lang tag makes a lot more sense than the class tag for <code> elements.
		width: 0, // Ignored when generating HTML
		default_info_string: None,
		unsafe_: config.markdown.enable_raw_html_inlining,
		ext_strikethrough: config.markdown.enable_github_extensions,
		ext_tagfilter: false,
		ext_table: config.markdown.enable_github_extensions,
		ext_autolink: config.markdown.enable_github_extensions,
		ext_tasklist: config.markdown.enable_github_extensions,
		ext_superscript: config.markdown.enable_comrak_extensions,
		ext_header_ids: None,
		ext_footnotes: config.markdown.enable_comrak_extensions,
		ext_description_lists: config.markdown.enable_comrak_extensions,
        }).as_bytes().to_vec();

	run_plugins(html_output, "html", config)
}

fn load_parse_write(fpath: &std::path::Path, config: &Config) -> Result<(), Box<dyn std::error::Error>> {
	let markdown_input = fs::read_to_string(&fpath)?;
	let html_output = parse_to_html(markdown_input, config)?;

	let mut file = File::create(fpath.with_extension("html"))?;
	if config.plugins.enable_core {
		file.write_all(b"<!doctype html><meta name=viewport content=\"width=device-width,initial-scale=1\">")?;
	}
	file.write_all(&html_output)?;
	Ok(())
}

fn main() {
	println!("Loading config...");
	let config_input = fs::read_to_string("conf.toml").unwrap_or_else(|_| {
		println!("Unable to read config file!");
		exit(exitcode::NOINPUT);
	});
	let config: Config = toml::from_str(&config_input).unwrap_or_else(|err| {
		println!("Unable to parse config file! Additional info below:\n{:#?}", err);
		exit(exitcode::CONFIG);
	});

	let _ = run_plugins(config_input.as_bytes().to_vec(), "config", &config).unwrap_or_else(|err| {
		println!("Unable to initialize plugins! Additional info below:\n{:#?}", err);
		exit(exitcode::UNAVAILABLE);
	});

	// This *should* never give an error, so using unwrap() here is fine.
	let files: Vec<_> = glob("./*.md").unwrap().filter_map(Result::ok).collect();

	files.par_iter().for_each(|fpath| {
		println!("Parsing {}...", fpath.to_string_lossy());
		if let Err(err) = load_parse_write(fpath, &config) {
			println!("Unable to parse {}! Additional info below:\n{:#?}", fpath.to_string_lossy(), err);
		}
	});
}
