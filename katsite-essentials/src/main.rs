#![warn(clippy::nursery)]
#![warn(clippy::pedantic)]
#![allow(clippy::cargo)]
#![warn(clippy::all)]

extern crate exitcode;
extern crate glob;
extern crate htmlescape;
extern crate liquid;
extern crate serde_derive;
extern crate toml;
use glob::glob;
use htmlescape::{encode_minimal, encode_attribute};
use liquid::ParserBuilder;
use serde_derive::{Serialize, Deserialize};
use std::{env, fs, io, time::UNIX_EPOCH, io::Read, process::exit, path::Path};

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

#[derive(Serialize)]
struct Page {
	modified_time: u64,
	name: String,
	basename: String,
}

#[derive(Serialize)]
struct Site {
	custom_css: Vec<String>,
	hometxt: String,
	pages: Vec<Page>,
}

static CSS_LOADER: &str = "{% for file in site.custom_css %}<link rel=stylesheet href=\"themes/{{ file }}\">{% endfor %}\n\n";
static BASIC_NAVBAR: &str = "\n\n[{{ site.hometxt }}](index.html){% for page in site.pages %}{% if page.basename == \"index\" %}{% continue %}{% endif %}\n[{{ page.basename }}]({{ page.name }}){% endfor %}\n\n---\n\n";
static COMPLEX_NAVBAR: &str = "\n\n<nav>\n{% if page.basename == \"index\" %}<a class=active id=home href=index.html><p>{{ site.hometxt }}</p></a>{% else %}<a id=home href=index.html><p>{{ site.hometxt }}</p></a>{% endif %}\n{% for page_ in site.pages %}{% if page_.basename == \"index\" %}{% continue %}{% endif %}{% if page_.name == page.name %}<a class=active href=\"{{ page_.name }}\"><p>{{ page_.basename }}</p></a>{% else %}<a href=\"{{ page_.name }}\"><p>{{ page_.basename }}</p></a>\n{% endif %}{% endfor %}\n</nav>\n\n";

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

// TODO: Improve error handling.
fn load_pageinfo<P: AsRef<Path>>(path: P) -> Page {
	let file = &path.as_ref();
	Page {
		modified_time: {
			fs::metadata(file).unwrap()
				.modified().unwrap()
				.duration_since(UNIX_EPOCH).unwrap()
				.as_secs()
		},
		name: {
			encode_attribute(
				&file.with_extension("html")
					.file_name().unwrap().to_string_lossy()
			)
		},
		basename: {
			encode_minimal(
				&file.file_stem().unwrap().to_string_lossy()
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
		hometxt: config.katsite_essentials.theme_hometxt,
		pages,
	}
}

fn render_liquid(data: &str, site: &Site, page: &Page) {
	let template = ParserBuilder::with_stdlib()
		.build().unwrap()
		.parse(data).unwrap();

	let globals = liquid::object!({
		"page": page,
		"site": site,
	});

	template.render_to(&mut io::stdout().lock(), &globals).unwrap();
}

fn render_markdown_page(config: Config, themeconfig: ThemeConfig, file: &str, data: &str) {
	let add_start = match themeconfig.layout_type {
		0 => {
			[CSS_LOADER, &themeconfig.append_top_html].concat()
		}
		1 => {
			[CSS_LOADER, &themeconfig.append_top_html, BASIC_NAVBAR].concat()
		}
		2 => {
			[CSS_LOADER, &themeconfig.append_top_html, COMPLEX_NAVBAR].concat()
		}
		x if themeconfig.append_top_html.is_empty() || x == 3 => {
			[CSS_LOADER, &themeconfig.append_top_html, COMPLEX_NAVBAR, "<article>\n\n"].concat()
		}
		_ => {
			[CSS_LOADER, "\n<header>\n", &themeconfig.append_top_html, COMPLEX_NAVBAR, "</header>\n<article>\n\n"].concat()
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
			exit(0);
		},
		_ => {
			eprintln!("KatSite Essentials is a plugin for KatSite, and is not meant to be used directly.");
			exit(exitcode::USAGE);
		},
	}
}
