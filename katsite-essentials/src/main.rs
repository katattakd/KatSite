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
extern crate sass_rs;
extern crate serde_derive;
extern crate toml;
extern crate urlencoding;
use ammonia::clean;
use brotli::enc::writer::CompressorWriter;
use glob::glob;
use htmlescape::encode_attribute;
use hyperbuild::hyperbuild_truncate;
use liquid::ParserBuilder;
use rayon::prelude::*;
use sass_rs::{compile_file, OutputStyle::Compressed};
use serde_derive::{Serialize, Deserialize};
use std::{env, fs, fs::File, io, ffi::OsStr, time::{Duration, UNIX_EPOCH}, io::{Read, Write}, process::exit, path::{Path, PathBuf}};
use urlencoding::encode;

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
	url_stub: String,

	default_lang: String,
	default_og_type: String,
	default_is_nsfw: bool,
	default_allow_robots: bool,

	stylesheet: String,

	layout: String,
	liquid_glob: String,

	sanitizer: bool,
	minifier: bool,
	brotli: bool,
}

#[derive(Deserialize)]
struct FrontMatter {
	title: Option<String>,
	description: Option<String>,
	locale: Option<String>,
	is_nsfw: Option<bool>,
	allow_robots: Option<bool>,
	og_type: Option<String>,
	og_image: Option<String>,
	og_audio: Option<String>,
	og_video: Option<String>,
}

#[derive(Serialize)]
struct Page {
	created_time: u64,
	modified_time: u64,
	filename: String,
	filename_url: String,
	filename_raw: String,
	data: String,
	title: String,
	description: Option<String>,
	locale: String,
	is_nsfw: bool,
	allow_robots: bool,
	og_type: String,
	og_image: Option<String>,
	og_audio: Option<String>,
	og_video: Option<String>,
}

#[derive(Serialize)]
struct Site {
	name: String,
	url_stub: String,
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
		filename: encode_attribute(&encode(&file_name)),
		filename_url: encode(&file_name),
		filename_raw: file_name.to_string(),
		data: contents,
		title: {
			let title = if let Some(title) = frontmatter.title {
				title
			} else if file_name == "index.html" {
				config.katsite_essentials.name.to_string()
			} else {
				file_stem.to_string()
			};
			if title.chars().count() > 65 {
				eprintln!("Warning: {}'s title is excessively long.", path.to_string_lossy())
			}
			encode_attribute(&title)
		},
		description: {
			if let Some(description) = frontmatter.description {
				if description.chars().count() > 155 {
					eprintln!("Warning: {}'s description is excessively long.", path.to_string_lossy())
				}
				Some(encode_attribute(&description))
			} else {
				None
			}
		},
		locale: encode_attribute(&frontmatter.locale.unwrap_or_else(|| config.katsite_essentials.default_lang.to_owned())),
		is_nsfw: frontmatter.is_nsfw.unwrap_or(config.katsite_essentials.default_is_nsfw),
		allow_robots: frontmatter.allow_robots.unwrap_or(config.katsite_essentials.default_allow_robots),
		og_type: encode_attribute(&frontmatter.og_type.unwrap_or_else(|| config.katsite_essentials.default_og_type.to_owned())),
		og_image: {
			if let Some(image) = frontmatter.og_image {
				Some(encode_attribute(&encode(&image)))
			} else {
				None
			}
		},
		og_audio: {
			if let Some(audio) = frontmatter.og_audio {
				Some(encode_attribute(&encode(&audio)))
			} else {
				None
			}
		},
		og_video: {
			if let Some(video) = frontmatter.og_video {
				Some(encode_attribute(&encode(&video)))
			} else {
				None
			}
		},
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
	pages.par_sort_unstable_by_key(|a| a.title.to_owned());

	Site {
		name: encode_attribute(&config.katsite_essentials.name),
		url_stub: config.katsite_essentials.url_stub.to_owned(),
		pages,
	}
}

fn load_additional_templates(site: &Site, config: &Config) {
	let files = glob(&config.katsite_essentials.liquid_glob).unwrap_or_else(|err| {
		eprintln!("Unable to create file glob! Additional info below:\n{:#?}", err);
		exit(exitcode::CONFIG);
	}).par_bridge();

	files.filter_map(Result::ok).for_each(|file| {
		if file.file_name() == Some(OsStr::new(&config.katsite_essentials.layout)) {
			return
		}

		eprintln!("Formatting {}...", file.file_stem().unwrap().to_string_lossy());

		let layout = fs::read_to_string(&file).unwrap_or_else(|_| {
			eprintln!("Unable to open {:#?}!", file);
			exit(exitcode::IOERR)
		});

		let template = ParserBuilder::with_stdlib()
			.build().unwrap_or_else(|err| {
				eprintln!("Unable to create liquid parser! Additional info below:\n{:#?}", err);
				exit(exitcode::SOFTWARE);
			})
			.parse(&layout).unwrap_or_else(|err| {
				eprintln!("Unable to parse {:#?}! Additional info below:\n{:#?}", file, err);
				exit(exitcode::DATAERR);
			});


		let globals = liquid::object!({
			"site": site,
		});

		let output = template.render(&globals).unwrap_or_else(|err| {
			eprintln!("Unable to render {:#?}! Additional info below:\n{:#?}", file, err);
			exit(exitcode::DATAERR);
		});

		fs::write(config.files.output_dir.join(&file.file_stem().unwrap()), output).unwrap_or_else(|_| {
			eprintln!("Unable to create {:#?}", file.file_stem());
			exit(exitcode::IOERR);
		});
	})
}

fn main() {
	let command = env::args().nth(1);

	match command {
		Some(x) if x == "markdown" => {
			let mut stdin = Vec::new();
			io::stdin().lock().read_to_end(&mut stdin).unwrap();
			io::stdout().lock().write_all(&stdin).unwrap();
		},
		Some(x) if x == "asyncinit" => {
			let config = load_config();

			println!("Compiling {}...", config.katsite_essentials.stylesheet);

			let output = compile_file(&config.katsite_essentials.stylesheet, sass_rs::Options{
				output_style: Compressed,
				precision: 2,
				indented_syntax: false,
				include_paths: vec![],
			}).unwrap_or_else(|err| {
				eprintln!("Unable to parse {:#?}! Additional info below:\n{:#?}", &config.katsite_essentials.stylesheet, err);
				exit(exitcode::DATAERR);
			});

			fs::write(config.files.output_dir.join("style.css"), output).unwrap_or_else(|_| {
				eprintln!("Unable to write stylesheet!");
				exit(exitcode::IOERR);
			});

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

			load_additional_templates(&site, &config);

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
