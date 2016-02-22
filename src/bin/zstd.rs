extern crate zstd;

use std::env;
use std::fs;
use std::io;

const SUFFIX: &'static str = ".zst";

fn main() {
    for arg in env::args().skip(1) {
        if arg.ends_with(SUFFIX) {
            match decompress(&arg) {
                Ok(()) => println!("Decompressed {}", arg),
                Err(e) => println!("Error decompressing {}: {}", arg, e),
            }
        } else {
            match compress(&arg) {
                Ok(()) => println!("Compressed {}", arg),
                Err(e) => println!("Error compressing {}: {}", arg, e),
            }
        }
    }
}

fn compress(source: &str) -> io::Result<()> {
    let mut file = try!(fs::File::open(source));
    let mut encoder = {
        let target = try!(fs::File::create(source.to_string() + SUFFIX));
        try!(zstd::Encoder::new(target, 1))
    };

    try!(io::copy(&mut file, &mut encoder));
    try!(encoder.finish());

    Ok(())
}

fn decompress(source: &str) -> io::Result<()> {
    let mut decoder = {
        let file = try!(fs::File::open(source));
        try!(zstd::Decoder::new(file))
    };

    let mut target = try!(fs::File::create(source.trim_right_matches(SUFFIX)));

    try!(io::copy(&mut decoder, &mut target));

    Ok(())
}
