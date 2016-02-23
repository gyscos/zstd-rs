extern crate zstd;

use std::env;
use std::str::FromStr;
use std::io::{self, Write};

fn main() {
    match env::args().skip(1).next() {
        None => { writeln!(&mut io::stderr(), "Invalid option. Usage: `stream [-d|-1..-21]`").unwrap(); },
        Some(ref option) if option == "-d" => decompress(),
        Some(ref option) => if option.starts_with("-") {
            let level = match i32::from_str(&option[1..]) {
                Ok(level) => level,
                Err(e) => panic!("Error parsing compression level: {}", e),
            };
            compress(level);
        } else {
            writeln!(&mut io::stderr(), "Invalid option. Usage: `stream [-d|-1..-21]`").unwrap();
        },
    }
}

fn compress(level: i32) {
	let mut encoder = zstd::Encoder::new(io::stdout(), level).unwrap();
	io::copy(&mut io::stdin(), &mut encoder).unwrap();
    encoder.finish().unwrap();
}

fn decompress() {
	let mut decoder = zstd::Decoder::new(io::stdin()).unwrap();
	io::copy(&mut decoder, &mut io::stdout()).unwrap();
}
