#![warn(clippy::nursery)]
#![warn(clippy::pedantic)]
#![warn(clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]
#![allow(clippy::cargo_common_metadata)]
#![warn(clippy::all)]

extern crate ammonia;
extern crate brotli;
extern crate exitcode;
extern crate glob;
extern crate htmlescape;
extern crate hyperbuild;
extern crate liquid;
extern crate rayon;
extern crate serde_derive;
extern crate toml;
use ammonia::clean;
use brotli::enc::writer::CompressorWriter;
use glob::glob;
use htmlescape::{encode_minimal, encode_attribute};
use hyperbuild::hyperbuild_truncate;
use liquid::ParserBuilder;
use rayon::prelude::*;
use serde_derive::{Serialize, Deserialize};
use std::{env, fs, fs::File, io, ffi::OsStr, time::{Duration, UNIX_EPOCH}, io::{Read, Write}, process::exit, path::Path};

#[derive(Deserialize)]
struct Config {
	thread_pool_size: usize,
	markdown: Markdown,
	html: Html,
	plugins: Plugins,
	katsite_essentials: Plugin,
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

#[derive(Deserialize)]
struct Plugin {
	theme: String,
	homepage_title: String,
	config_checker: bool,
	liquid: bool,
	sanitizer: bool,
	minifier: bool,
	brotli: bool,
}

#[derive(Deserialize)]
struct ThemeConfig {
	css: Vec<String>,
	layout_type: usize,
	append_top_html: String,
	append_bottom_html: String,
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

fn load_siteinfo(config: &Config, themeconfig: ThemeConfig) -> Site {
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

fn render_markdown_page(config: &Config, themeconfig: ThemeConfig, file: &str, data: &str) {
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

			if config.katsite_essentials.liquid && !config.katsite_essentials.sanitizer {
				let themeconfig = load_theme_config(&config.katsite_essentials.theme);
				render_markdown_page(&config, themeconfig, &file.unwrap(), &input);
			} else {
				println!("{}", input);
			}
		},
		Some(x) if x == "asyncinit" => {
			let config = load_config();

			if !config.katsite_essentials.config_checker {
				exit(0);
			}

			println!("Checking config...");

			if config.markdown.convert_line_breaks {
				eprintln!("Warn: markdown.convert_line_breaks may mess with the rendering of many documents/themes, and it breaks the commonmark spec.")
			}

			if !config.html.append_5doctype {
				eprintln!("Warn: Disabling html.append_5doctype will result in output HTML that isn't spec compliant.")
			}

			if !config.html.append_viewport {
				eprintln!("Warn: Disabling html.append_viewport may cause graphical issues for mobile users.")
			}

			if config.thread_pool_size == 1 {
				eprintln!("Warn: Using a small thread pool size may significantly slow down KatSite.")
			}

			if config.plugins.plugins_list.len() > 5 {
				eprintln!("Warn: Using an excessive number of plugins may slow down KatSite.")
			}

			if !(config.katsite_essentials.minifier && config.katsite_essentials.liquid && config.katsite_essentials.sanitizer || config.plugins.plugins_list.len() > 3 || config.katsite_essentials.brotli || config.markdown.enable_comrak_extensions || config.markdown.enable_github_extensions || config.markdown.convert_line_breaks) {
				if config.markdown.convert_punctuation {
					eprintln!("Warn: Disabling markdown.convert_punctuation will allow KatSite to use a significantly faster Markdown parser.")
				}
				if config.markdown.create_header_anchors {
					eprintln!("Warn: Disabling markdown.create_header_anchors will allow KatSite to use a significantly faster Markdown parser.")
				}
			}

			if !config.katsite_essentials.minifier && config.katsite_essentials.brotli {
				eprintln!("Warn: Enabling katsite_essentials.minifier may significantly reduce the size of the output html.")
			}

			if config.katsite_essentials.minifier && config.markdown.create_header_anchors {
				eprintln!("Warn: Disabling markdown.create_header_anchors may significantly reduce the size of the output HTML.")
			}

			if config.katsite_essentials.theme != "none" && !(config.katsite_essentials.liquid || config.katsite_essentials.sanitizer) {
				eprintln!("Warn: Disabling katsite_essentials.liquid will automatically disable theming.")
			}

			if config.katsite_essentials.liquid && config.katsite_essentials.sanitizer {
				eprintln!("Warn: Enabling katsite_essentials.sanitizer will automatically disable liquid templating.")
			}
		},
		Some(x) if x == "postinit" => {
			let config = load_config();
			if !(config.katsite_essentials.minifier || config.katsite_essentials.sanitizer || config.katsite_essentials.brotli) {
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
				let mut input = fs::read(&fpath).unwrap_or_else(|_| {
					eprintln!("Unable to open {:#?}!", &fpath);
					exit(exitcode::NOINPUT);
				});

				if config.katsite_essentials.sanitizer {
					println!("Sanitizing {}...", fpath.to_string_lossy());
					let mut append = String::new();
					if config.html.append_5doctype {
						append = "<!doctype html>".to_string()
					}
					if config.html.append_viewport {
						append = [&append, "<meta name=viewport content=\"width=device-width,initial-scale=1\">"].concat()
					}

					input = [append, clean(&String::from_utf8_lossy(&input))].concat().into_bytes();
				}

				if config.katsite_essentials.minifier {
					println!("Minifying {}...", fpath.to_string_lossy());
					hyperbuild_truncate(&mut input).unwrap_or_else(|err| {
						eprintln!("Unable to minify {:#?}! Additional info below:\n{:#?}", &fpath, err);
						exit(exitcode::DATAERR);
					});
				}

				fs::write(&fpath, &input).unwrap_or_else(|_| {
					eprintln!("Unable to write to {:#?}!", &fpath);
					exit(exitcode::IOERR);
				});

				if config.katsite_essentials.brotli {
					println!("Compressing {}...", fpath.to_string_lossy());
					let mut file = File::create(&fpath.with_extension("html.br")).unwrap_or_else(|_| {
						eprintln!("Unable to open {:#?}!", &fpath);
						exit(exitcode::IOERR);
					});
					CompressorWriter::new(&mut file, 4096, 11, 24).write_all(&input).unwrap_or_else(|_| {
						eprintln!("Unable to write to {:#?}!", &fpath);
						exit(exitcode::IOERR);
					});
				}
			})
		},
		_ => {
			eprintln!("KatSite Essentials is a plugin for KatSite, and is not meant to be used directly.");
			exit(exitcode::USAGE);
		},
	}
}
