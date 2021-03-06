#![warn(clippy::nursery)]
#![warn(clippy::pedantic)]
#![allow(clippy::struct_excessive_bools)]
#![allow(clippy::cast_possible_truncation)]
#![warn(clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]
#![allow(clippy::cargo_common_metadata)]
#![warn(clippy::all)]

use ammonia::clean;
use brotli::enc::{writer::CompressorWriter, backward_references::{BrotliEncoderParams, BrotliHasherParams, BrotliEncoderMode}, command::BrotliDistanceParams, encode::{BROTLI_MAX_DISTANCE, BROTLI_MAX_DISTANCE_BITS, BROTLI_DISTANCE_ALPHABET_SIZE}};
use extract_frontmatter::Extractor;
use glob::glob;
use htmlescape::encode_attribute;
use image::{imageops::FilterType::Lanczos3, math::nq::NeuQuant, imageops::colorops::ColorMap};
use liquid::ParserBuilder;
use minify_html::{Cfg, truncate};
use oxipng::{optimize, InFile, OutFile, Options, Headers::All, Deflaters::Zopfli};
use rayon::prelude::*;
use sass_rs::{compile_file, OutputStyle::Expanded};
use serde_derive::{Serialize, Deserialize};
use std::{env, fs, fs::File, io, ffi::OsStr, time::{Duration, UNIX_EPOCH}, io::{Read, Write}, process::{exit, Command, Stdio}, path::{Path, PathBuf}, thread};
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

	layout: PathBuf,
	liquid_glob: String,
	stylesheet: PathBuf,
	favicon: PathBuf,

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

fn compress_file(path: &Path, mode: BrotliEncoderMode) {
	let mut input_file = File::open(&path).unwrap_or_else(|_| {
		eprintln!("Unable to open {:#?}!", path.to_string_lossy());
		exit(exitcode::IOERR);
	});
	let input_size = input_file.metadata().map(|m| m.len() as usize).unwrap_or(0);
	let mut input = Vec::with_capacity(input_size + 1);
	input_file.read_to_end(&mut input).unwrap_or_else(|_| {
		eprintln!("Unable to read {:#?}!", path.to_string_lossy());
		exit(exitcode::IOERR);
	});

	let output_path = path.with_extension([&path.extension().unwrap_or_else(|| OsStr::new("")).to_string_lossy(), ".br"].concat());
	let mut output = File::create(&output_path).unwrap_or_else(|_| {
		eprintln!("Unable to create {:#?}!", path.with_extension(".br").to_string_lossy());
		exit(exitcode::IOERR);
	});
	let params = BrotliEncoderParams {
		dist: BrotliDistanceParams {
			distance_postfix_bits: 0,
			num_direct_distance_codes: 0,
			alphabet_size: BROTLI_DISTANCE_ALPHABET_SIZE(0, 0, BROTLI_MAX_DISTANCE_BITS),
			max_distance: BROTLI_MAX_DISTANCE,
		},
		mode,
		quality: 11,
		q9_5: false,
		lgwin: 24,
		lgblock: 0,
		size_hint: input_size,
		disable_literal_context_modeling: 0,
		hasher: BrotliHasherParams {
			type_: 6,
			block_bits: 11 - 1,
			bucket_bits: 15,
			hash_len: 5,
			num_last_distances_to_check: 16,
			literal_byte_score: 0,
		},
		log_meta_block: false,
		stride_detection_quality: 0,
		high_entropy_detection_quality: 0,
		cdf_adaptation_detection: 0,
		prior_bitmask_detection: 0,
		literal_adaptation: [(0,0);4],
		large_window: false,
		avoid_distance_prefix_search:false,
		catable: false,
		use_dictionary: true,
		appendable: false,
		magic_number: false,
		favor_cpu_efficiency: false,
	};
	CompressorWriter::with_params(&mut output, 4096, &params).write_all(&input).unwrap_or_else(|_| {
		eprintln!("Unable to write to {:#?}!", path.with_extension(".br").to_string_lossy());
		exit(exitcode::IOERR);
	});
	if output.metadata().map(|m| m.len() as usize).unwrap_or(0) > input_size {
		let _ = fs::remove_file(output_path);
	}
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
		Extractor::new(&contents)
			.select_by_terminator("-->")
			.discard_first_line()
			.extract()
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

		let output_path = config.files.output_dir.join(&file.file_stem().unwrap());

		fs::write(&output_path, output).unwrap_or_else(|_| {
			eprintln!("Unable to create {:#?}", file.file_stem());
			exit(exitcode::IOERR);
		});

		if !config.katsite_essentials.brotli {
			return
		}

		println!("Compressing {}...", file.file_stem().unwrap().to_string_lossy());
		compress_file(&output_path, BrotliEncoderMode::BROTLI_MODE_TEXT);
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

			let minifier = config.katsite_essentials.minifier;
			let brotli = config.katsite_essentials.brotli;
			let output_dir = config.files.output_dir.to_owned();
			let stylesheet = config.katsite_essentials.stylesheet.to_owned();
			let thread = thread::spawn(move || {
				if !stylesheet.exists() {
					return
				}

				println!("Compiling {}...", stylesheet.to_string_lossy());

				let output = compile_file(&stylesheet, sass_rs::Options{
					output_style: Expanded,
					precision: 2,
					indented_syntax: false,
					include_paths: vec![],
				}).unwrap_or_else(|err| {
					eprintln!("Unable to parse {:#?}! Additional info below:\n{:#?}", &stylesheet, err);
					exit(exitcode::DATAERR);
				});

				let output_file = output_dir.join("style.css");
				fs::write(&output_file, output).unwrap_or_else(|_| {
					eprintln!("Unable to write stylesheet!");
					exit(exitcode::IOERR);
				});

				if minifier {
					println!("Minifying {}...", stylesheet.to_string_lossy());
					let mut child = Command::new("csso")
						.arg(&output_file)
						.arg("--output").arg(&output_file)
						.stdin(Stdio::null())
						.stdout(Stdio::inherit())
						.stderr(Stdio::inherit())
						.spawn().unwrap_or_else(|err| {
							eprintln!("Unable to start CSS minifier! Additional info below:\n{}", err);
							exit(exitcode::UNAVAILABLE);
					});
					let _ = child.wait();
				}

				if !brotli {
					return
				}

				println!("Compressing {}...", stylesheet.to_string_lossy());
				compress_file(&output_file, BrotliEncoderMode::BROTLI_MODE_TEXT);
			});

			if config.katsite_essentials.favicon.exists() {
				println!("Parsing {}...", config.katsite_essentials.favicon.to_string_lossy());
				let icon1 = image::open(&config.katsite_essentials.favicon).unwrap_or_else(|_| {
					eprintln!("Unable to read {:#?}!", config.katsite_essentials.favicon);
					exit(exitcode::NOINPUT);
				});
				let icon2 = icon1.to_owned();

				let output1 = config.files.output_dir.join("apple-touch-icon.png");
				let output2 = config.files.output_dir.join("favicon.png");

				let mut options1 = Options::from_preset(6);
				options1.fix_errors = true;
				options1.strip = All;
				options1.deflate = Zopfli;
				let options2 = options1.to_owned();

				let thread1 = thread::spawn(move || {
					println!("Creating apple-touch-icon.png...");

					let mut icon = icon1.resize_to_fill(192, 192, Lanczos3).to_rgba();

					let nq = NeuQuant::new(1, 64, icon.to_owned().into_flat_samples().as_slice());
					for pixel in icon.pixels_mut() {
						nq.map_color(pixel);
					}

					icon.save(output1.to_owned()).unwrap_or_else(|_| {
						eprintln!("Unable to create apple-touch-icon.png!");
						exit(exitcode::CANTCREAT);
					});

					if minifier {
						println!("Minifying apple-touch-icon.png...");
						optimize(&InFile::Path(output1.to_owned()), &OutFile::Path(None), &options1)
						.unwrap_or_else(|_| {
							eprintln!("Unable to minify apple-touch-icon.png!");
							exit(exitcode::IOERR);
						});
					}

					if !brotli {
						return
					}

					println!("Compressing apple-touch-icon.png...");
					compress_file(&output1, BrotliEncoderMode::BROTLI_MODE_GENERIC);
				});
				
				let thread2 = thread::spawn(move || {
					println!("Creating favicon.png...");

					let mut icon = icon2.resize_to_fill(48, 48, Lanczos3).to_rgba();

					let nq = NeuQuant::new(1, 16, icon.to_owned().into_flat_samples().as_slice());
					for pixel in icon.pixels_mut() {
						nq.map_color(pixel);
					}

					icon.save(output2.to_owned()).unwrap_or_else(|_| {
						eprintln!("Unable to create favicon.png!");
						exit(exitcode::CANTCREAT);
					});

					if minifier {
						println!("Minifying favicon.png...");
						optimize(&InFile::Path(output2.to_owned()), &OutFile::Path(None), &options2)
						.unwrap_or_else(|_| {
							eprintln!("Unable to minify favicon.png!");
							exit(exitcode::IOERR);
						});
					}

					if !brotli {
						return
					}

					println!("Compressing favicon.png...");
					compress_file(&output2, BrotliEncoderMode::BROTLI_MODE_GENERIC);
				});

				let _ = thread1.join();
				let _ = thread2.join();
			}

			let _ = thread.join();
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
					let cfg = &Cfg {
						minify_js: true,
					};
					truncate(&mut input, cfg).unwrap_or_else(|err| {
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
					compress_file(&path, BrotliEncoderMode::BROTLI_MODE_TEXT);
				}
			})
		},
		_ => {
			eprintln!("KatSite Essentials is a plugin for KatSite, and is not meant to be used directly.");
			exit(exitcode::USAGE);
		},
	}
}
