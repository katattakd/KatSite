#![warn(clippy::nursery)]
#![warn(clippy::pedantic)]
#![warn(clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]
#![allow(clippy::cargo_common_metadata)]
#![warn(clippy::all)]

extern crate ammonia;
extern crate brotli;
extern crate exitcode;
extern crate extract_frontmatter;
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
use std::{env, fs, fs::File, io, ffi::OsStr, time::{Duration, UNIX_EPOCH}, io::{Read, Write}, process::exit, path::{Path, PathBuf}};

#[derive(Deserialize)]
struct Config {
	files: Files,
	katsite_essentials: Plugin,
}

#[derive(Deserialize)]
struct Files {
	input_glob: String,
	output_dir: PathBuf,
}

#[derive(Deserialize)]
struct Plugin {
	name: String,

	og_stub: String,
	default_lang: String,
	default_og_type: String,
	default_is_nsfw: bool,
	default_allow_robots: bool,

	theme: String,
	layout: String,

	sanitizer: bool,
	minifier: bool,
	brotli: bool,
}

#[derive(Deserialize)]
struct FrontMatter {
	title: Option<String>, // Warn if over 65 char
	description: Option<String>, // Warn if over 155 char
	locale: Option<String>, // Default to default_lang
	is_nsfw: Option<bool>, // Default to false
	allow_robots: Option<bool>, // Default to true
	og_type: Option<String>, // Default to default_og_type
	og_image: Option<String>,
}

#[derive(Serialize)]
struct Page {
	created_time: u64,
	modified_time: u64,
	name: String,
	filename: String,
	filename_raw: String,
	data: String,
	title: String,
	description: Option<String>,
	locale: String,
	is_nsfw: bool,
	allow_robots: bool,
	og_type: String,
	og_image: Option<String>,
}

#[derive(Serialize)]
struct Site {
	name: String,
	theme: String,
	og_stub: String,
	pages: Vec<Page>,
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

fn load_pageinfo<P: AsRef<Path>>(config: &Config, path: P) -> Page {
	let path = &path.as_ref();
	let metadata = path.metadata();

	let html_file = path.with_extension("html");
	let file_stem = path.file_stem().unwrap_or_else(|| {
		path.extension().unwrap_or_else(|| OsStr::new(".html"))
	}).to_string_lossy();
	let file_name = html_file.file_name().unwrap().to_string_lossy();

	let mut contents = fs::read_to_string(config.files.output_dir.join(&html_file)).unwrap_or_else(|_| {
		eprintln!("Unable to open {:#?}!", path);
		exit(exitcode::NOINPUT);
	});

	let frontmatter_str = if contents.starts_with("<!--") {
		extract_frontmatter::extract(
			&extract_frontmatter::Config::new(None, Some("-->"), None, true),
			&contents
		)
	} else {
		"".to_string()
	};

	if config.katsite_essentials.sanitizer {
		println!("Sanitizing {}...", path.to_string_lossy());
		contents = clean(&contents);
	}

	let frontmatter: FrontMatter = toml::from_str(&frontmatter_str).unwrap_or_else(|err| {
		eprintln!("Unable to parse {:#?}'s frontmatter! Additional info below:\n{:#?}", path, err);
		exit(exitcode::DATAERR);
	});

	Page {
		created_time: {
			if let Ok(meta) = &metadata {
				meta.created().unwrap_or(UNIX_EPOCH)
				.duration_since(UNIX_EPOCH).unwrap_or_else(|_| Duration::new(0, 0))
				.as_secs()
			} else {
				0
			}
		},
		modified_time: {
			if let Ok(meta) = &metadata {
				meta.modified().unwrap_or(UNIX_EPOCH)
				.duration_since(UNIX_EPOCH).unwrap_or_else(|_| Duration::new(0, 0))
				.as_secs()
			} else {
				0
			}
		},
		name: encode_minimal(&file_stem),
		filename: encode_attribute(&file_name),
		filename_raw: file_name.to_string(),
		data: contents,
		title: {
			let title = if let Some(title) = frontmatter.title {
				title
			} else {
				if path.file_name() == Some(OsStr::new("index.md")) {
					config.katsite_essentials.name.to_owned()
				} else {
					[&file_stem, " - ", &config.katsite_essentials.name].concat()
				}
			};
			encode_attribute(&title)
		},
		description: {
			if let Some(description) = frontmatter.description {
				Some(encode_attribute(&description))
			} else {
				None
			}
		},
		locale: encode_attribute(&frontmatter.locale.unwrap_or(config.katsite_essentials.default_lang.to_owned())),
		is_nsfw: frontmatter.is_nsfw.unwrap_or(config.katsite_essentials.default_is_nsfw),
		allow_robots: frontmatter.allow_robots.unwrap_or(config.katsite_essentials.default_allow_robots),
		og_type: encode_attribute(&frontmatter.og_type.unwrap_or(config.katsite_essentials.default_og_type.to_owned())),
		og_image: {
			if let Some(image) = frontmatter.og_image {
				Some(encode_attribute(&image))
			} else {
				None
			}
		}
	}
}

fn load_siteinfo(config: &Config) -> Site {
	let files = glob(&config.files.input_glob).unwrap_or_else(|err| {
		eprintln!("Unable to create file glob! Additional info below:\n{:#?}", err);
		exit(exitcode::CONFIG);
	}).par_bridge();

	let mut pages: Vec<Page> = files.filter_map(Result::ok).map(|file| {
		load_pageinfo(config, &file)
	}).collect();
	pages.par_sort_unstable_by_key(|a| a.filename_raw.to_owned());

	Site {
		name: config.katsite_essentials.name.to_owned(),
		theme: config.katsite_essentials.theme.to_owned(),
		og_stub: config.katsite_essentials.og_stub.to_owned(),
		pages: pages,
	}
}
fn main() {
	let command = env::args().nth(1);

	match command {
		Some(x) if x == "markdown" => {
			let mut stdin = Vec::new();
			io::stdin().lock().read_to_end(&mut stdin).unwrap();
			io::stdout().lock().write_all(&mut stdin).unwrap();
		},
		Some(x) if x == "asyncinit" => {
			exit(0);
		},
		Some(x) if x == "postinit" => {
			let config = load_config();

			println!("Creating site template...");

			let layout = fs::read_to_string(&config.katsite_essentials.layout).unwrap_or_else(|_| {
				eprintln!("Unable to open template file!");
				exit(exitcode::NOINPUT)
			});

			let template = ParserBuilder::with_stdlib()
				.build().unwrap_or_else(|err| {
					eprintln!("Unable to create liquid parser! Additional info below:\n{:#?}", err);
					exit(exitcode::SOFTWARE);
				})
				.parse(&layout).unwrap_or_else(|err| {
					eprintln!("Unable to parse template! Additional info below:\n{:#?}", err);
					exit(exitcode::DATAERR);
				});

			let site = load_siteinfo(&config);
			site.pages.par_iter().for_each(|page| {
				println!("Formatting {}...", page.filename_raw);

				let globals = liquid::object!({
					"page": page,
					"site": site,
				});

				let mut input = template.render(&globals).unwrap_or_else(|err| {
					eprintln!("Unable to render template! Additional info below:\n{:#?}", err);
					exit(exitcode::DATAERR);
				}).into_bytes();

				if config.katsite_essentials.minifier {
					println!("Minifying {}...", page.filename_raw);
					hyperbuild_truncate(&mut input).unwrap_or_else(|err| {
						eprintln!("Unable to minify {:#?}! Additional info below:\n{:#?}", page.filename_raw, err);
						exit(exitcode::DATAERR);
					});
				}

				let path = config.files.output_dir.join(&page.filename_raw);

				fs::write(&path, &input).unwrap_or_else(|_| {
					eprintln!("Unable to write to {:#?}!", page.filename_raw);
					exit(exitcode::IOERR);
				});

				if config.katsite_essentials.brotli {
					println!("Compressing {}...", page.filename_raw);
					let mut file = File::create(
						Path::new(&path).with_extension("html.br")
					).unwrap_or_else(|_| {
						eprintln!("Unable to open {:#?}!", page.filename_raw);
						exit(exitcode::IOERR);
					});
					CompressorWriter::new(&mut file, 4096, 11, 24).write_all(&input).unwrap_or_else(|_| {
						eprintln!("Unable to write to {:#?}!", page.filename);
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
