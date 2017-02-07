extern crate gcc;
extern crate glob;

#[cfg(not(feature = "legacy"))]
fn set_legacy(_: &mut gcc::Config) {
}

#[cfg(feature = "legacy")]
fn set_legacy(config: &mut gcc::Config) {
    config.define("ZSTD_LEGACY_SUPPORT", Some("1"));
}

fn main() {
    let mut config = gcc::Config::new();

    let globs = &["zstd/lib/common/*.c",
                  "zstd/lib/compress/*.c",
                  "zstd/lib/decompress/*.c",
                  "zstd/lib/legacy/*.c",
                  "zstd/lib/deprecated/*.c",
                  "zstd/lib/dictBuilder/*.c"];

    for pattern in globs {
        for path in glob::glob(pattern).unwrap() {
            let path = path.unwrap();
            config.file(path);
        }
    }

    // Some extra parameters
    config.opt_level(3);
    config.include("zstd/lib/");
    config.include("zstd/lib/common");
    config.include("zstd/lib/legacy");

    set_legacy(&mut config);

    // Compile!
    config.compile("libzstd.a");
}
