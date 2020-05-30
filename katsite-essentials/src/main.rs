#![warn(clippy::nursery)]
#![warn(clippy::pedantic)]
#![allow(clippy::cargo)]
#![warn(clippy::all)]

extern crate exitcode;
extern crate glob;
extern crate serde_derive;
extern crate toml;
use glob::glob;
use serde_derive::Deserialize;
use std::{env, fs, io, io::{Read, Write}, process::exit};

#[derive(Deserialize)]
struct Config {
	katsite_essentials: Plugin,
}

#[derive(Deserialize)]
struct ThemeConfig {
	css: Vec<String>,
	layout_type: usize,
	append_top_html: String,
	append_bottom_html: String,
}

#[derive(Deserialize)]
struct Plugin {
	theme: String,
	theme_hometxt: String,
	minifier: bool,
}

fn load_config() -> Config {
	let config_input = fs::read_to_string("conf.toml").unwrap_or_else(|_| {
		println!("Unable to read config file!");
		exit(exitcode::NOINPUT)
	});
	toml::from_str(&config_input).unwrap_or_else(|err| {
		println!("Unable to parse config file! Additional info below:\n{:#?}", err);
		exit(exitcode::CONFIG);
	})
}

fn render_navbar(navtype: usize, input_filename: String, hometext: String) {
	if navtype == 0 {
		return
	}

	if navtype >= 2 {
		println!("<nav>");
	}

	if navtype == 1 {
		println!("\n[{}](index.html)", hometext);
	} else if input_filename == "index.md" {
		println!("<a class=active href=index.html>\n\n{}\n\n</a>", hometext);
	} else {
		println!("<a href=index.html>\n\n{}\n\n</a>", hometext);
	}

	for file in glob("./*.md").unwrap() {
		let file = file.unwrap();
		let filepath = file.with_extension("html");
		if filepath.to_string_lossy() == "index.html" {
			continue
		}

		let filename = file.file_stem().unwrap_or_else(|| file.extension().unwrap()).to_string_lossy();
		
		if [&filename, ".md"].concat() == input_filename && navtype >= 2 {
			println!("<a class=active href=\"{}\"><p>{}</p></a>", filepath.to_string_lossy(), filename);
			continue
		}

		if navtype >= 2 {
			println!("<a href=\"{}\"><p>{}</p></a>", filepath.to_string_lossy(), filename);
		} else {
			println!("[{}]({})", filename, filepath.to_string_lossy());
		}
	}

	println!("{}", match navtype {
		1 => "\n---\n",
		_ => "</nav>\n",
	});
}

fn load_theme_config(theme: String) -> ThemeConfig {
	if theme == "none" {
		ThemeConfig {
			css: vec![],
			layout_type: 0,
			append_top_html: "".to_string(),
			append_bottom_html: "".to_string(),
		}
	} else {
		let config_input = fs::read_to_string(["themes/theme-", &theme, ".toml"].concat()).unwrap_or_else(|_| ["css=[\"theme-", &theme, ".css\"]\nlayout_type=1\nappend_top_html=\"\"\nappend_bottom_html=\"\""].concat());
		toml::from_str(&config_input).expect("Unable to parse theme config!")
	}
}

fn main() {
	let command = env::args().nth(1);

	match command {
		Some(x) if x == "markdown" => {
			let config = load_config();
			let file = env::args().nth(2).unwrap();

			let mut stdin = Vec::new();
			io::stdin().lock().read_to_end(&mut stdin).unwrap();

			let tconfig = load_theme_config(config.katsite_essentials.theme);

			for file in tconfig.css {
				println!("<link rel=stylesheet href=\"themes/{}\">\n", file);
			}
			if tconfig.layout_type >= 4 && !tconfig.append_top_html.is_empty() {
				println!("<header>\n");
			}

			println!("{}\n", tconfig.append_top_html);
			render_navbar(tconfig.layout_type, file, config.katsite_essentials.theme_hometxt);

			if tconfig.layout_type >= 4 && !tconfig.append_top_html.is_empty() {
				println!("\n</header>");
			}

			if tconfig.layout_type >= 3 {
				println!("<article>\n");
			}

			io::stdout().lock().write_all(&mut stdin).unwrap();

			if tconfig.layout_type >= 3 {
				println!("\n</article>");
			}

			if tconfig.layout_type >= 4 && !tconfig.append_bottom_html.is_empty() {
				println!("<footer>");
			}

			println!("\n{}\n", tconfig.append_bottom_html);

			if tconfig.layout_type >= 4 && !tconfig.append_bottom_html.is_empty() {
				println!("</footer>\n");
			}
		},
		Some(x) if x == "asyncinit" => {
			exit(0);
		},
		Some(x) if x == "postinit" => {
			exit(0);
		},
		_ => {
			println!("KatSite Essentials is a plugin for KatSite, and is not meant to be used directly.");
			exit(exitcode::USAGE);
		},
	}
}
