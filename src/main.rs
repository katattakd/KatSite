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
use std::{thread, string::String, path::PathBuf, fs, fs::File, io::{copy, Error, Cursor, Read, Write, BufWriter}, process::{exit, Command, Stdio, Output}};

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

fn init_plugins(hook: &str, list: Vec<String>) {
	list.par_iter().for_each(|plugin| {
		let mut child = Command::new(PathBuf::from("plugins/").join(plugin))
			.arg(hook)
			.stdin(Stdio::null())
			.stdout(Stdio::null())
			.stderr(Stdio::inherit())
			.spawn().unwrap_or_else(|err| {
				println!("Unable to start plugin {}! Additional info below:\n{}", plugin, err);
				exit(exitcode::OSERR);
			});
		let _ = child.wait();
	})
}

// TODO: Improve error handling
fn run_plugins(mut buffer: &mut Vec<u8>, hook: &str, config: &Config) {
	for plugin in &config.plugins.plugins_list {
		let mut child = Command::new(PathBuf::from("plugins/").join(plugin))
			.arg(hook)
			.stdin(Stdio::piped())
			.stdout(Stdio::piped())
			.stderr(Stdio::inherit())
			.spawn().unwrap_or_else(|err| {
				println!("Unable to start plugin {}! Additional info below:\n{}", plugin, err);
				exit(exitcode::UNAVAILABLE);
			});

		child.stdin.as_mut().unwrap()
			.write_all(&buffer).unwrap_or_else(|_| {
				println!("Plugin {} crashed during usage!", plugin);
				exit(exitcode::UNAVAILABLE);
			});

		buffer.clear();
		drop(child.stdin.take());

		child.stdout.as_mut().unwrap()
			.read_to_end(&mut buffer).unwrap_or_else(|_| {
				println!("Plugin {} crashed during usage!", plugin);
				exit(exitcode::UNAVAILABLE);
			});

		let _ = child.kill();
	}
}

fn markdown_to_html(input: &str, output: &mut dyn Write, config: &Config) -> Result<(), Error> {
	if config.markdown.filter_html_tags || config.markdown.convert_line_breaks || config.markdown.convert_punctuation || config.markdown.enable_github_extensions || config.markdown.enable_comrak_extensions || !config.markdown.enable_raw_html_inlining {
		let arena = &Arena::new();
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
		let root = parse_document(arena, input, options);
		format_html(root, options, output)
	} else {
		let parser = Parser::new(input);
		html::write_html(output, parser)
	}
}

// TODO: Improve error handling.
fn parse_to_file(input: &mut Vec<u8>, output: &mut dyn Write, config: &Config) -> Result<(), Error> {
	let mut output = BufWriter::new(output);
	if config.html.append_5doctype {
		output.write_all(b"<!doctype html>")?;
	}
	if config.html.append_viewport {
		output.write_all(b"<meta name=viewport content=\"width=device-width,initial-scale=1\">")?;
	}

	run_plugins(input, "markdown", config);
	let mk_input = std::str::from_utf8(&input).unwrap_or_else(|_| {
		println!("Invalid UTF-8 output from plugin!");
		exit(exitcode::DATAERR);
	});

	markdown_to_html(&mk_input, &mut output, config)?;

	output.flush()
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

	let files = glob("./*.md").unwrap_or_else(|err| {
		println!("Unable to create file glob! Additional info below:\n{:#?}", err);
		exit(exitcode::SOFTWARE);
	}).par_bridge();

	let plugins_list = config.plugins.plugins_list.to_owned();
	thread::spawn(move || {
		init_plugins("init", plugins_list);
	});

	files.filter_map(Result::ok).for_each(|fpath| {
		println!("Parsing {}...", fpath.to_string_lossy());
		let mut input = fs::read(&fpath).unwrap_or_else(|_| {
			println!("Unable to open {:#?}!", &fpath);
			exit(exitcode::NOINPUT);
		});

		let output_name = fpath.with_extension("html");
		let mut output = File::create(&output_name).unwrap_or_else(|_| {
			println!("Unable to create {:#?}!", &output_name);
			exit(exitcode::CANTCREAT);
		});

		parse_to_file(&mut input, &mut output, &config).unwrap_or_else(|_| {
			println!("Unable to finish parsing {:#?}!", &fpath);
			exit(exitcode::IOERR);
		});
	});

	println!("Finishing up...");
	init_plugins("postinit", config.plugins.plugins_list);
}
