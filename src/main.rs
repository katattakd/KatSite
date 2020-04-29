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
use std::{string::String, path::PathBuf, fs, io::Write, process::{exit, Command, Stdio, Output}};

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

fn run_plugin(plugin: &str, hook: &str, input: Option<std::vec::Vec<u8>>) -> Output {
	let mut child = Command::new(PathBuf::from("plugins/").join(plugin))
		.arg(hook)
		.stdin(Stdio::piped())
		.stdout(Stdio::piped())
		.stderr(Stdio::inherit())
		.spawn().unwrap_or_else(|err| {
			println!("Unable to start plugin {}! Additional info below:\n{}", plugin, err);
			exit(exitcode::OSERR);
		});

	if let Some(raw_input) = input {
		// TODO: Check if it's possible for this unwrap() call to fail.
		let _ = child.stdin.as_mut().unwrap().write_all(&raw_input);
	}

	child.wait_with_output().unwrap_or_else(|err| {
		println!("Unable to get output of plugin {}! Additional info below:\n{}", plugin, err);
		exit(exitcode::OSERR);
	})
}

fn init_plugins(hook: &str, config: &Config) {
	config.plugins.plugins_list.par_iter().for_each(|plugin| {
		let exit_status = run_plugin(plugin, hook, None).status;
		if !exit_status.success() {
			println!("Warn: Plugin {} returned a non-zero exit code during init.", plugin);
		}
	});
}

fn run_plugins(mut input: std::vec::Vec<u8>, hook: &str, config: &Config) -> std::vec::Vec<u8> {
	for plugin in &config.plugins.plugins_list {
		let output = run_plugin(plugin, hook, Some(input.to_owned()));
		if output.status.success() {
			if !output.stdout.is_empty() {
				input = output.stdout;
			}
		} else {
			println!("Warn: Plugin {} returned a non-zero exit code, discarding it's output...", plugin);
		}
	}
	input
}

fn parse_to_html(raw_input: std::vec::Vec<u8>, config: &Config) -> std::vec::Vec<u8> {
	let plugin_output = run_plugins(raw_input, "markdown", config);
	let markdown_input = String::from_utf8_lossy(&plugin_output).to_string();

	let mut html_output = markdown_to_html(&markdown_input, &ComrakOptions {
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
        });

	if config.plugins.enable_core {
		html_output += "<!doctype html><meta name=viewport content=\"width=device-width,initial-scale=1\">";
	}

	run_plugins(html_output.as_bytes().to_vec(), "html", config)
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

	println!("Initializing plugins...");
	init_plugins("config", &config);

	let files = glob("./*.md").unwrap().par_bridge(); // Should never give an error, so we can safely use unwrap().
	files.filter_map(Result::ok).for_each(|fpath| {
		println!("Parsing {}...", fpath.to_string_lossy());
		let raw_input = fs::read(&fpath).unwrap_or_else(|_| {
			println!("Unable to read {:#?}!", &fpath);
			exit(exitcode::NOINPUT);
		});

		let output = parse_to_html(raw_input, &config);

		let output_name = fpath.with_extension("html");
		fs::write(&output_name, output).unwrap_or_else(|_| {
			println!("Unable to create {:#?}!", &output_name);
			exit(exitcode::CANTCREAT);
		});
	});

	println!("Finishing up...");
	init_plugins("postconfig", &config);
}
