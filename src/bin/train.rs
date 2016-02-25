extern crate zstd;

use std::env;
use std::fs;
use std::str::FromStr;
use std::io::{self, Read, Write};

fn main() {
    let mut buffer = Vec::new();
    let mut sample_sizes = Vec::new();

    let mut ignore_options = false;
    let mut max_size = 110 * 1024;

    let mut expect_size = false;

    if env::args().len() == 1 {
        writeln!(io::stderr(), "Usage: `train [-c MAX_SIZE] FILES...`").unwrap();
        return;
    }

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
