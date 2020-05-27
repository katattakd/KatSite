#![deny(clippy::nursery)]
#![deny(clippy::pedantic)]
#![allow(clippy::cargo)]
#![deny(clippy::all)]

extern crate comrak;
extern crate exitcode;
extern crate glob;
extern crate pulldown_cmark;
extern crate rayon;
extern crate serde_derive;
extern crate toml;
use comrak::{Arena, parse_document, format_html, ComrakOptions};
use glob::glob;
use pulldown_cmark::{Parser, html};
use rayon::prelude::*;
use serde_derive::Deserialize;
use std::{string::String, path::PathBuf, fs, io::Write, process::{exit, Command, Stdio, Output}};

#[derive(Deserialize)]
struct Config {
	thread_pool_size: usize,
	markdown: Markdown,
	html: Html,
	plugins: Plugins,
}

#[derive(Deserialize)]
struct Markdown {
	filter_html_tags: bool,
	convert_line_breaks: bool,
	convert_punctuation: bool,
	enable_raw_html_inlining: bool,
	enable_github_extensions: bool,
	enable_comrak_extensions: bool,
}

#[derive(Deserialize)]
struct Html {
	append_5doctype: bool,
	append_viewport: bool,
}

#[derive(Deserialize)]
struct Plugins {
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
	if config.plugins.plugins_list.is_empty() {
		return
	}
	config.plugins.plugins_list.par_iter().for_each(|plugin| {
		let exit_status = run_plugin(plugin, hook, None).status;
		if !exit_status.success() {
			println!("Warn: Plugin {} returned a non-zero exit code during init.", plugin);
		}
	});
}

fn run_plugins(mut input: std::vec::Vec<u8>, hook: &str, config: &Config) -> std::vec::Vec<u8> {
	if config.plugins.plugins_list.is_empty() {
		return input
	}
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
	let markdown_input = &String::from_utf8_lossy(&plugin_output);
	
	let mut html_output: Vec<u8> = Vec::new();

	if config.markdown.filter_html_tags || config.markdown.convert_line_breaks || config.markdown.convert_punctuation || config.markdown.enable_github_extensions || config.markdown.enable_comrak_extensions || !config.markdown.enable_raw_html_inlining {
		let arena = Arena::new();
		let options = &ComrakOptions {
			hardbreaks: config.markdown.convert_line_breaks,
			smart: config.markdown.convert_punctuation,
			github_pre_lang: true, // The lang tag makes a lot more sense than the class tag for <code> elements.
			width: 0, // Ignored when generating HTML
			default_info_string: None,
			unsafe_: config.markdown.enable_raw_html_inlining,
			ext_strikethrough: config.markdown.enable_github_extensions,
			ext_tagfilter: config.markdown.filter_html_tags,
			ext_table: config.markdown.enable_github_extensions,
			ext_autolink: config.markdown.enable_github_extensions,
			ext_tasklist: config.markdown.enable_github_extensions,
			ext_superscript: config.markdown.enable_comrak_extensions,
			ext_header_ids: None,
			ext_footnotes: config.markdown.enable_comrak_extensions,
			ext_description_lists: config.markdown.enable_comrak_extensions,
		};
		let root = parse_document(&arena, markdown_input, options);
		format_html(root, options, &mut html_output).unwrap();
	} else {
		let parser = Parser::new(markdown_input);
		html::write_html(&mut html_output, parser).unwrap();
	};

	run_plugins(html_output, "html", config)
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

	rayon::ThreadPoolBuilder::new().num_threads(config.thread_pool_size).build_global().unwrap_or_else(|err| {
		println!("Unable to create thread pool! Additional info below:\n{:#?}", err);
		exit(exitcode::OSERR);
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
		let mut f = fs::File::create(&output_name).unwrap_or_else(|_| {
			println!("Unable to create {:#?}!", &output_name);
			exit(exitcode::CANTCREAT);
		});
		if config.html.append_5doctype {
			f.write(b"<!doctype html>").unwrap();
		}
		if config.html.append_viewport {
			f.write(b"<meta name=viewport content=\"width=device-width,initial-scale=1\">").unwrap();
		}
		f.write(&output).unwrap();
	});

	println!("Finishing up...");
	init_plugins("postconfig", &config);
}
