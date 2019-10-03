use clap::{value_t, App, Arg};
use std::io;

// This program trains a dictionary from one or more files,
// to make future compression of similar small files more efficient.
//
// The dictionary will need to be present during decompression,
// but if you need to compress many small files individually,
// it may be worth the trouble.
fn main() {
    let matches = App::new("train")
        .author("Alexandre Bury <alexandre.bury@gmail.com>")
        .about("A zstd dict trainer")
        .arg(
            Arg::with_name("MAX_SIZE")
                .help("Maximum dictionary size in bytes")
                .short("s")
                .long("max_size")
                .takes_value(true),
        )
        .arg(
            Arg::with_name("FILE")
                .help("Files to use as input")
                .required(true)
                .multiple(true),
        )
        .get_matches();

    let size = value_t!(matches, "MAX_SIZE", usize).unwrap_or(110 * 1024);

    let files: Vec<_> = matches.values_of("FILE").unwrap().collect();

    let dict = zstd::dict::from_files(&files, size).unwrap();

    let mut dict_reader: &[u8] = &dict;
    io::copy(&mut dict_reader, &mut io::stdout()).unwrap();
}
