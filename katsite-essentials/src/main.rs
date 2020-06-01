#![warn(clippy::nursery)]
#![warn(clippy::pedantic)]
#![allow(clippy::cargo)]
#![warn(clippy::all)]

extern crate exitcode;
extern crate glob;
extern crate htmlescape;
extern crate hyperbuild;
extern crate liquid;
extern crate rayon;
extern crate serde_derive;
extern crate toml;
use glob::glob;
use htmlescape::{encode_minimal, encode_attribute};
use hyperbuild::hyperbuild_truncate;
use liquid::ParserBuilder;
use rayon::prelude::*;
use serde_derive::{Serialize, Deserialize};
use std::{env, fs, io, ffi::OsStr, time::{Duration, UNIX_EPOCH}, io::Read, process::exit, path::Path};

#[derive(Deserialize)]
struct Config {
	thread_pool_size: usize,
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
	homepage_title: String,
	minifier: bool,
}

#[derive(Serialize)]
struct Page {
	modified_time: u64,
	name: String,
	basename: String,
}

#[derive(Serialize)]
struct Site {
	custom_css: Vec<String>,
	home_title: String,
	pages: Vec<Page>,
}

static CSS_LOADER: &str = "{% for file in site.custom_css %}<link rel=stylesheet href=\"themes/{{ file }}\">{% endfor %}\n\n";
static TITLE_LOADER: &str = "<title>{% if page.basename == \"index\" %}{{ site.home_title }}{% else %}{{ page.basename }}{% endif %}</title>\n\n";
static BASIC_NAVBAR: &str = "\n\n[Home](index.html){% for page in site.pages %}{% if page.basename == \"index\" %}{% continue %}{% endif %}\n| [{{ page.basename }}]({{ page.name }}){% endfor %}\n\n---\n\n";
static COMPLEX_NAVBAR: &str = "\n\n<nav>\n{% if page.basename == \"index\" %}<a class=active href=index.html><h3>{{ site.home_title }}</h3></a>{% else %}<a href=index.html><h3>{{ site.home_title }}</h3></a>{% endif %}\n{% for page_ in site.pages %}{% if page_.basename == \"index\" %}{% continue %}{% endif %}{% if page_.name == page.name %}<a class=active href=\"{{ page_.name }}\"><p>{{ page_.basename }}</p></a>{% else %}<a href=\"{{ page_.name }}\"><p>{{ page_.basename }}</p></a>\n{% endif %}{% endfor %}\n</nav>\n\n";

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
				"css=[\"theme-", theme,
				".css\"]\nlayout_type=1\nappend_top_html=\"\"\nappend_bottom_html=\"\""
			].concat());
		toml::from_str(&config_input).unwrap_or_else(|err| {
			eprintln!("Unable to parse theme config file! Additional info below:\n{:#?}", err);
			exit(exitcode::CONFIG);
		})
	}
}

fn load_pageinfo<P: AsRef<Path>>(path: P) -> Page {
	let file = &path.as_ref();
	Page {
		modified_time: {
			if let Ok(meta) = file.metadata() {
				meta.modified().unwrap_or(UNIX_EPOCH)
				.duration_since(UNIX_EPOCH).unwrap_or_else(|_| Duration::new(0, 0))
				.as_secs()
			} else {
				0
			}
		},
		name: {
			encode_attribute(
				&file.with_extension("html")
					.file_name().unwrap_or_else(|| OsStr::new("")).to_string_lossy()
			)
		},
		basename: {
			encode_minimal(
				&file.file_stem().unwrap_or_else(|| {
					file.extension().unwrap_or_else(|| OsStr::new(".html"))
				}).to_string_lossy()
			)
		},
	}
}

fn load_siteinfo(config: Config, themeconfig: ThemeConfig) -> Site {
	let files = glob("./*.md").unwrap_or_else(|err| {
		eprintln!("Unable to create file glob! Additional info below:\n{:#?}", err);
		exit(exitcode::SOFTWARE);
	});

	let mut pages = Vec::new();
	files.filter_map(Result::ok).for_each(|file| {
		pages.push(
			load_pageinfo(&file)
		);
	});

	Site {
		custom_css: themeconfig.css,
		home_title: encode_minimal(&config.katsite_essentials.homepage_title),
		pages,
	}
}

fn render_liquid(data: &str, site: &Site, page: &Page) {
	let template = ParserBuilder::with_stdlib()
		.build().unwrap_or_else(|err| {
			eprintln!("Unable to create liquid parser! Additional info below:\n{:#?}", err);
			exit(exitcode::SOFTWARE);
		})
		.parse(data).unwrap_or_else(|err| {
			eprintln!("Unable to parse input! Additional info below:\n{:#?}", err);
			exit(exitcode::DATAERR);
		});

	let globals = liquid::object!({
		"page": page,
		"site": site,
	});

	template.render_to(&mut io::stdout().lock(), &globals).unwrap_or_else(|err| {
		eprintln!("Unable to render liquid input! Additional info below:\n{:#?}", err);
		exit(exitcode::DATAERR);
	});
}

fn render_markdown_page(config: Config, themeconfig: ThemeConfig, file: &str, data: &str) {
	let add_start = match themeconfig.layout_type {
		0 => {
			[CSS_LOADER, TITLE_LOADER, &themeconfig.append_top_html].concat()
		}
		1 => {
			[CSS_LOADER, TITLE_LOADER, &themeconfig.append_top_html, BASIC_NAVBAR].concat()
		}
		2 => {
			[CSS_LOADER, TITLE_LOADER, &themeconfig.append_top_html, COMPLEX_NAVBAR].concat()
		}
		x if themeconfig.append_top_html.is_empty() || x == 3 => {
			[CSS_LOADER, TITLE_LOADER, &themeconfig.append_top_html, COMPLEX_NAVBAR, "<article>\n\n"].concat()
		}
		_ => {
			[CSS_LOADER, TITLE_LOADER, "\n<header>\n", &themeconfig.append_top_html, COMPLEX_NAVBAR, "</header>\n<article>\n\n"].concat()
		}
	};

	let add_end = match themeconfig.layout_type {
		0 | 1 | 2 => {
			(&themeconfig.append_bottom_html).to_string()
		},
		x if themeconfig.append_bottom_html.is_empty() || x == 3 => {
			["\n</article>\n", &themeconfig.append_bottom_html].concat()
		}
		_ => {
			["\n</article>\n<footer>\n", &themeconfig.append_bottom_html, "\n</footer>"].concat()
		},
	};

	let page = load_pageinfo(file);
	let site = load_siteinfo(config, themeconfig);

	render_liquid(&[&add_start, "\n\n", data, "\n\n", &add_end].concat(), &site, &page);
}

fn main() {
	let command = env::args().nth(1);
	let file = env::args().nth(2);

	match command {
		Some(x) if x == "markdown" => {
			let mut input = String::new();
			io::stdin().lock().read_to_string(&mut input).unwrap();

			let config = load_config();
			let themeconfig = load_theme_config(&config.katsite_essentials.theme);

			render_markdown_page(config, themeconfig, &file.unwrap(), &input);
		},
		Some(x) if x == "asyncinit" => {
			exit(0);
		},
		Some(x) if x == "postinit" => {
			let config = load_config();
			if !config.katsite_essentials.minifier {
				exit(0);
			}

			rayon::ThreadPoolBuilder::new().num_threads(config.thread_pool_size).build_global().unwrap_or_else(|err| {
				eprintln!("Unable to create thread pool! Additional info below:\n{:#?}", err);
				exit(exitcode::OSERR);
			});

			let files = glob("./*.html").unwrap_or_else(|err| {
				eprintln!("Unable to create file glob! Additional info below:\n{:#?}", err);
				exit(exitcode::SOFTWARE);
			}).par_bridge();

			files.filter_map(Result::ok).for_each(|fpath| {
				println!("Minifying {}...", fpath.to_string_lossy());
				let mut input = fs::read(&fpath).unwrap_or_else(|_| {
					eprintln!("Unable to open {:#?}!", &fpath);
					exit(exitcode::NOINPUT);
				});

				hyperbuild_truncate(&mut input).unwrap_or_else(|err| {
					eprintln!("Unable to minify {:#?}! Additional info below:\n{:#?}", &fpath, err);
					exit(exitcode::DATAERR);
				});

				fs::write(&fpath, input).unwrap_or_else(|_| {
					eprintln!("Unable to write to {:#?}!", &fpath);
					exit(exitcode::IOERR);
				})
			})
		},
		_ => {
			eprintln!("KatSite Essentials is a plugin for KatSite, and is not meant to be used directly.");
			exit(exitcode::USAGE);
		},
	}
}
