#![warn(clippy::nursery)]
#![warn(clippy::pedantic)]
#![warn(clippy::cargo)]
#![allow(clippy::cargo_common_metadata)]
#![allow(clippy::multiple_crate_versions)]
#![warn(clippy::all)]

use image::{imageops::FilterType::Lanczos3, math::nq::NeuQuant, imageops::colorops::ColorMap};
use oxipng::{optimize, InFile, OutFile, Options, Headers::All, Deflaters::Zopfli};
use serde_derive::Deserialize;
use std::{thread, env, fs, process::exit, path::PathBuf, io, io::{Read, Write}};

#[derive(Deserialize)]
struct Config {
	files: Files,
	katsite_favicons: Plugin,
}

#[derive(Deserialize)]
struct Files {
	output_dir: PathBuf,
}

#[derive(Deserialize)]
struct Plugin {
	favicon: PathBuf,
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
			let config_input = fs::read_to_string("conf.toml").unwrap_or_else(|_| {
				eprintln!("Unable to read config file!");
				exit(exitcode::NOINPUT)
			});
			let config: Config = toml::from_str(&config_input).unwrap_or_else(|err| {
				eprintln!("Unable to parse config file! Additional info below:\n{:#?}", err);
				exit(exitcode::CONFIG);
			});

			if config.katsite_favicons.favicon.exists() {
				println!("Parsing {}...", config.katsite_favicons.favicon.to_string_lossy());
				let icon1 = image::open(&config.katsite_favicons.favicon).unwrap_or_else(|_| {
					eprintln!("Unable to read {:#?}!", config.katsite_favicons.favicon);
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

					println!("Minifying apple-touch-icon.png...");
					optimize(&InFile::Path(output1), &OutFile::Path(None), &options1).unwrap_or_else(|_| {
						eprintln!("Unable to minify apple-touch-icon.png!");
						exit(exitcode::IOERR);
					});
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

					println!("Minifying favicon.png...");
					optimize(&InFile::Path(output2), &OutFile::Path(None), &options2).unwrap_or_else(|_| {
						eprintln!("Unable to minify favicon.png!");
						exit(exitcode::IOERR);
					});
				});

				let _ = thread1.join();
				let _ = thread2.join();
			}
		},
		Some(x) if x == "postinit" => {
			exit(0);
		},
		_ => {
			eprintln!("KatSite Favicons is a plugin for KatSite, and is not meant to be used directly.");
			exit(exitcode::USAGE);
		},
	}
}
