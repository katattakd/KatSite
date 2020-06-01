#![warn(clippy::nursery)]
#![warn(clippy::pedantic)]
#![warn(clippy::cargo)]
#![allow(clippy::cargo_common_metadata)]
#![warn(clippy::all)]

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
use std::{thread, string::String, path::PathBuf, fs, fs::File, io::{Error, Read, Write, BufWriter}, process::{exit, Command, Stdio}};

#[derive(Deserialize)]
struct Config {
	thread_pool_size: usize,
	markdown: Markdown,
	html: Html,
	plugins: Plugins,
}

#[derive(Deserialize)]
struct Markdown {
	convert_line_breaks: bool,
	convert_punctuation: bool,
	create_header_anchors: bool,
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

fn init_plugins(hook: &str, list: &[String]) {
	list.par_iter().for_each(|plugin| {
		let mut child = Command::new(PathBuf::from("plugins/").join(plugin))
			.arg(hook)
			.stdin(Stdio::null())
			.stdout(Stdio::inherit())
			.stderr(Stdio::inherit())
			.spawn().unwrap_or_else(|err| {
				eprintln!("Unable to start plugin {}! Additional info below:\n{}", plugin, err);
				exit(exitcode::OSERR);
			});
		let _ = child.wait();
	})
}

fn run_plugins(mut buffer: &mut Vec<u8>, hook: &str, filename: &str, config: &Config) {
	for plugin in &config.plugins.plugins_list {
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

fn markdown_to_html(input: &str, output: &mut dyn Write, config: &Config) -> Result<(), Error> {
	if config.markdown.convert_line_breaks || config.markdown.convert_punctuation || config.markdown.enable_github_extensions || config.markdown.enable_comrak_extensions || config.markdown.create_header_anchors {
		let arena = &Arena::new();
		let headerids = if config.markdown.create_header_anchors {
			Some("".to_string())
		} else {
			None
		};
		let options = &ComrakOptions {
			hardbreaks: config.markdown.convert_line_breaks,
			smart: config.markdown.convert_punctuation,
			github_pre_lang: true, // The lang tag makes a lot more sense than the class tag for <code> elements.
			width: 0, // Ignored when generating HTML
			default_info_string: None,
			unsafe_: true, // Not worth disabling, a proper HTML sanitizer should be used instead.
			ext_strikethrough: config.markdown.enable_github_extensions,
			ext_tagfilter: false, // Not worth enabling, a proper HTML sanitizer should be used instead.
			ext_table: config.markdown.enable_github_extensions,
			ext_autolink: config.markdown.enable_github_extensions,
			ext_tasklist: config.markdown.enable_github_extensions,
			ext_superscript: config.markdown.enable_comrak_extensions,
			ext_header_ids: headerids,
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

fn parse_to_file(input: &mut Vec<u8>, output: &mut dyn Write, filename: &str, config: &Config) -> Result<(), Error> {
	let mut output = BufWriter::new(output);
	if config.html.append_5doctype {
		output.write_all(b"<!doctype html>")?;
	}
	if config.html.append_viewport {
		output.write_all(b"<meta name=viewport content=\"width=device-width,initial-scale=1\">")?;
	}

	run_plugins(input, "markdown", filename, config);
	markdown_to_html(&String::from_utf8_lossy(input), &mut output, config)?;

	output.flush()
}

fn main() {
	println!("Loading config...");
	let config_input = fs::read_to_string("conf.toml").unwrap_or_else(|_| {
		eprintln!("Unable to read config file!");
		exit(exitcode::NOINPUT);
	});
	let config: Config = toml::from_str(&config_input).unwrap_or_else(|err| {
		eprintln!("Unable to parse config file! Additional info below:\n{:#?}", err);
		exit(exitcode::CONFIG);
	});

	rayon::ThreadPoolBuilder::new().num_threads(config.thread_pool_size).build_global().unwrap_or_else(|err| {
		eprintln!("Unable to create thread pool! Additional info below:\n{:#?}", err);
		exit(exitcode::OSERR);
	});

	let files = glob("./*.md").unwrap_or_else(|err| {
		eprintln!("Unable to create file glob! Additional info below:\n{:#?}", err);
		exit(exitcode::SOFTWARE);
	}).par_bridge();

	let plugins_list = config.plugins.plugins_list.to_owned();
	let child = thread::spawn(move || {
		init_plugins("asyncinit", &plugins_list);
	});

	files.filter_map(Result::ok).for_each(|fpath| {
		let input_name = fpath.to_string_lossy();

		println!("Parsing {}...", input_name);
		let mut input = fs::read(&fpath).unwrap_or_else(|_| {
			eprintln!("Unable to open {:#?}!", &fpath);
			exit(exitcode::NOINPUT);
		});

		let output_name = fpath.with_extension("html");
		let mut output = File::create(&output_name).unwrap_or_else(|_| {
			eprintln!("Unable to create {:#?}!", &output_name);
			exit(exitcode::CANTCREAT);
		});

		parse_to_file(&mut input, &mut output, &input_name, &config).unwrap_or_else(|_| {
			eprintln!("Unable to finish parsing {:#?}!", &fpath);
			exit(exitcode::IOERR);
		});
	});

	init_plugins("postinit", &config.plugins.plugins_list);
	let _ = child.join();
}
