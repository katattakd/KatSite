#![deny(clippy::nursery)]
#![deny(clippy::pedantic)]
#![allow(clippy::cargo)]
#![deny(clippy::all)]

extern crate ammonia;
extern crate comrak;
extern crate glob;
extern crate hyperbuild;
extern crate rayon;
extern crate serde_derive;
extern crate toml;
use comrak::{markdown_to_html, ComrakOptions};
use glob::glob;
use hyperbuild::hyperbuild;
use rayon::prelude::*;
use serde_derive::Deserialize;
use std::{fs, fs::File, io::Write};

#[derive(Deserialize)]
struct Config {
	markdown: Markdown,
	sanitizer: Sanitizer,
	minifier: Minifier,
	html: HTML,
}

#[derive(Deserialize)]
struct Markdown {
	convert_line_breaks: bool,
	convert_punctuation: bool,
	enable_inline_html: bool,
	enable_github_extensions: bool,
	enable_comrak_extensions: bool,
}

#[derive(Deserialize)]
struct Sanitizer {
	sanitize_generated: bool,
}

#[derive(Deserialize)]
struct Minifier {
	minify_custom: bool,
	minify_generated: bool,
}

#[derive(Deserialize)]
struct HTML {
	append_doctype: bool,
	append_viewport: bool,
	custom_css: String,
	custom_header_html: String,
	custom_footer_html: String,
}

fn parse_to_html(markdown_input: String, config: &Config) -> std::vec::Vec<u8> {
	let mut markdown_html_output = markdown_to_html(&markdown_input, &ComrakOptions {
		hardbreaks: config.markdown.convert_line_breaks,
		smart: config.markdown.convert_punctuation,
		github_pre_lang: true,
		width: 0,
		default_info_string: None,
		unsafe_: config.markdown.enable_inline_html,
		ext_strikethrough: config.markdown.enable_github_extensions,
		ext_tagfilter: config.sanitizer.sanitize_generated,
		ext_table: config.markdown.enable_github_extensions,
		ext_autolink: config.markdown.enable_github_extensions,
		ext_tasklist: config.markdown.enable_github_extensions,
		ext_superscript: config.markdown.enable_comrak_extensions,
		ext_header_ids: None,
		ext_footnotes: config.markdown.enable_comrak_extensions,
		ext_description_lists: config.markdown.enable_comrak_extensions,
        });

	if config.sanitizer.sanitize_generated {
		markdown_html_output = ammonia::Builder::default()
			.clean(&markdown_html_output)
			.to_string();
	}

	let mut html_output = markdown_html_output.as_bytes().to_vec();
	if config.minifier.minify_generated {
		let _ = hyperbuild(&mut html_output);
	}

	html_output
}

fn load_parse_write(fpath: &std::path::Path, header: &std::vec::Vec<u8>, footer: &std::vec::Vec<u8>, config: &Config) -> Result<(), Box<dyn std::error::Error>> {
	let markdown_input = fs::read_to_string(&fpath).unwrap();
	let html_output = parse_to_html(markdown_input, &config);

	let mut file = File::create([&fpath.file_stem().unwrap().to_string_lossy(), ".html"].concat()).unwrap();
	if config.html.append_doctype {
		file.write_all(b"<!DOCTYPE html>").unwrap();
	}
	if config.html.append_viewport {
		file.write_all(b"<meta name=viewport content=\"width=device-width,initial-scale=1\">").unwrap();
	}
	if !config.html.custom_css.is_empty() {
		file.write_all(b"<style>").unwrap();
		file.write_all(config.html.custom_css.as_bytes()).unwrap();
		file.write_all(b"</style>").unwrap();
	}
	if !header.is_empty() {
		file.write_all(&header).unwrap();
	}
	file.write_all(&html_output).unwrap();
	if !footer.is_empty() {
		file.write_all(&footer).unwrap();
	}
	Ok(())
}

fn main() {
	println!("Loading config...");
	let config_input = fs::read_to_string("conf.toml").unwrap();
	let config: Config = toml::from_str(&config_input).unwrap();

	let mut custom_header = config.html.custom_header_html.as_bytes().to_vec();
	let mut custom_footer = config.html.custom_footer_html.as_bytes().to_vec();
	if config.minifier.minify_custom {
		hyperbuild(&mut custom_header).unwrap();
		hyperbuild(&mut custom_footer).unwrap();
	}

	let files: Vec<_> = glob("./*.md").unwrap().filter_map(Result::ok).collect();

	files.par_iter().for_each(|fpath| {
		println!("Parsing {}...", fpath.to_string_lossy());
		load_parse_write(fpath, &custom_header, &custom_footer, &config).unwrap();
	});
}
