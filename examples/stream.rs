use std::env;
use std::io::{self, Write};
use std::str::FromStr;

fn main() {
    match env::args().nth(1) {
        None => {
            writeln!(
                &mut io::stderr(),
                "Invalid option. Usage: `stream [-d|-1..-22]`"
            )
            .unwrap();
        }
        Some(ref option) if option == "-d" => decompress(),
        Some(ref option) => match option.strip_prefix('-') {
            Some(level) => {
                let level = match i32::from_str(level) {
                    Ok(level) => level,
                    Err(e) => {
                        panic!("Error parsing compression level: {}", e)
                    }
                };
                compress(level);
            }
            None => {
                writeln!(
                    &mut io::stderr(),
                    "Invalid option. Usage: `stream [-d|-1..-22]`"
                )
                .unwrap();
            }
        },
    }
}

fn compress(level: i32) {
    zstd::stream::copy_encode(io::stdin(), io::stdout(), level).unwrap();
}

fn decompress() {
    zstd::stream::copy_decode(io::stdin(), io::stdout()).unwrap();
}
