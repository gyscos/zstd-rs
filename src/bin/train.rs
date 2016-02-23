extern crate zstd;

use std::env;
use std::fs;
use std::str::FromStr;
use std::io::{self, Read, Write};

// This program trains a dictionary from one or more files,
// to make future compression of similar small files more efficient.
//
// The dictionary will need to be present during decompression,
// but it you need to compress many small files individually,
// it may be worth the trouble.
fn main() {
    // We will concatenate all the input files in one big buffer.
    let mut buffer = Vec::new();
    // We'll keep track of each file size
    let mut sample_sizes = Vec::new();

    // Passing "--" treats everything after that as a filename
    let mut ignore_options = false;

    // A tiny state machine to parse the max size.
    let mut expect_size = false;
    let mut max_size = 110 * 1024;

    if env::args().len() == 1 {
        writeln!(io::stderr(), "Usage: `train [-c MAX_SIZE] FILES...`").unwrap();
        return;
    }

    // We do some ugly manual option parsing, because I'm trying to avoid using `clap`.
    // When cargo allows per-binary dependencies, this may get cleaner.
    for filename in env::args().skip(1) {
        if !ignore_options {
            match filename.as_ref() {
                "--" => {
                    ignore_options = true;
                    continue;
                }
                "-c" => {
                    expect_size = true;
                    continue;
                }
                other if expect_size => {
                    expect_size = false;
                    max_size = usize::from_str(other).expect("Invalid size");
                    continue;
                }
                _ => (),
            }
        }
        let mut file = fs::File::open(filename).unwrap();
        let size = file.read_to_end(&mut buffer).unwrap();
        sample_sizes.push(size);
    }

    let dict = zstd::dict::from_continuous(&buffer, &sample_sizes, max_size).unwrap();
    let mut dict_reader: &[u8] = &dict;
    io::copy(&mut dict_reader, &mut io::stdout()).unwrap();
}
