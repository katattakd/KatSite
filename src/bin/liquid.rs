extern crate liquid;
use std::{env, io, io::Read};

fn main() {
	if env::args().nth(1) != Some("markdown".to_string()) {
		return
	}

	let mut stdin = String::new();
	io::stdin().lock().read_to_string(&mut stdin).expect("Unable to read from stdin!");

	let template = liquid::ParserBuilder::with_stdlib()
		.build().expect("Unable to build liquid parser!")
		.parse(&stdin).expect("Unable to create liquid template!");
	template.render_to(
		&mut io::stdout().lock(),
		&liquid::Object::new()
	).expect("Unable to render liquid template!");
}
