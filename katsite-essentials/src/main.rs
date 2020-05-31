#![warn(clippy::nursery)]
#![warn(clippy::pedantic)]
#![allow(clippy::cargo)]
#![warn(clippy::all)]

extern crate exitcode;
extern crate glob;
extern crate htmlescape;
extern crate serde_derive;
extern crate toml;
use glob::glob;
use htmlescape::{encode_minimal, encode_attribute};
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
		eprintln!("Unable to read config file!");
		exit(exitcode::NOINPUT)
	});
	toml::from_str(&config_input).unwrap_or_else(|err| {
		eprintln!("Unable to parse config file! Additional info below:\n{:#?}", err);
		exit(exitcode::CONFIG);
	})
}

fn load_theme_config(theme: &str) -> ThemeConfig {
	if theme == "none" {
		ThemeConfig {
			css: vec![],
			layout_type: 0,
			append_top_html: "".to_string(),
			append_bottom_html: "".to_string(),
		}
	} else {
		let config_input = fs::read_to_string(["themes/theme-", theme, ".toml"].concat())
			.unwrap_or_else(|_| [
				"css=[\"theme-",
				theme,
				".css\"]\nlayout_type=1\nappend_top_html=\"\"\nappend_bottom_html=\"\""
			].concat());
		toml::from_str(&config_input).unwrap_or_else(|err| {
			eprintln!("Unable to parse theme config file! Additional info below:\n{:#?}", err);
			exit(exitcode::CONFIG);
		})
	}
}

fn render_navbar(navtype: usize, input_filename: &str, hometext: &str) {
	match navtype {
		0 => return,
		1 => println!("[{}](index.html)", htmlescape::encode_minimal(hometext)),
		_ => {
			println!("<nav>");
			if input_filename == "index.md" {
				print!("<a class=active ");
			} else {
				print!("<a ");
			}
			println!("id=home href=index.html><p>{}</p></a>", hometext); // Hometext is purposely unescaped.
		}
	}

	let files = glob("./*.md").unwrap_or_else(|err| {
		eprintln!("Unable to create file glob! Additional info below:\n{:#?}", err);
		exit(exitcode::SOFTWARE);
	});

	files.filter_map(Result::ok).for_each(|file| {
		let hfile = file.with_extension("html");
		let name = htmlescape::encode_minimal(
			&hfile.file_stem().unwrap_or_else(||
				file.extension().unwrap()
			).to_string_lossy()
		);
		let path = htmlescape::encode_attribute(&hfile.to_string_lossy());

		if name == "index" {
			return
		}

		if navtype == 1 {
			println!("[{}]({})", name, path)
		} else {
			if file.to_string_lossy() == input_filename {
				print!("<a class=active ");
			} else {
				print!("<a ");
			}
			println!("href=\"{}\"><p>{}</p></a>", path, name);
		}
	});

	println!("{}", if navtype == 1 {
		"\n---\n"
	} else {
		"</nav>\n"
	});
}

fn render_markdown_page(config: &Config, themeconfig: ThemeConfig, file: &str, data: &[u8]) {
	for file in themeconfig.css {
		println!("<link rel=stylesheet href=\"themes/{}\">", file);
	}

	if themeconfig.layout_type >= 4 && !themeconfig.append_top_html.is_empty() {
		println!("<header>");
	}

	println!("{}\n", themeconfig.append_top_html);

	render_navbar(themeconfig.layout_type, file, &config.katsite_essentials.theme_hometxt);

	if themeconfig.layout_type >= 4 && !themeconfig.append_top_html.is_empty() {
		println!("</header>");
	}

	if themeconfig.layout_type >= 3 {
		println!("<article>\n");
	}

	io::stdout().lock().write_all(data).unwrap();

	if themeconfig.layout_type >= 3 {
		println!("\n</article>");
	}

	if themeconfig.layout_type >= 4 && !themeconfig.append_bottom_html.is_empty() {
		println!("<footer>{}</footer>", themeconfig.append_bottom_html);
	} else {
		println!("{}", themeconfig.append_bottom_html);
	}
}

fn main() {
	let command = env::args().nth(1);
	let file = env::args().nth(2);

	match command {
		Some(x) if x == "markdown" => {
			let mut stdin = Vec::new();
			io::stdin().lock().read_to_end(&mut stdin).unwrap();

			let config = load_config();
			let themeconfig = load_theme_config(&config.katsite_essentials.theme);

			render_markdown_page(&config, themeconfig, &file.unwrap(), &stdin);
		},
		Some(x) if x == "asyncinit" => {
			exit(0);
		},
		Some(x) if x == "postinit" => {
			exit(0);
		},
		_ => {
			eprintln!("KatSite Essentials is a plugin for KatSite, and is not meant to be used directly.");
			exit(exitcode::USAGE);
		},
	}
}
