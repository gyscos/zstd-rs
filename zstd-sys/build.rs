#[cfg(feature = "bindgen")]
extern crate bindgen;

extern crate gcc;
extern crate glob;


#[cfg(feature = "bindgen")]
fn generate_bindings() {
    use std::env;
    use std::path::PathBuf;

    let bindings = bindgen::Builder::default()
        .no_unstable_rust()
        .header("zstd.h")
        .generate_comments(false) //< remove this when it works
        .hide_type("max_align_t")
        .use_core()
        .ctypes_prefix("::libc")
        .clang_arg("-DZSTD_STATIC_LINKING_ONLY")
        .generate()
        .expect("Unable to generate bindings");

    let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());
    bindings.write_to_file(out_path.join("bindings.rs")).expect("Could not write bindings");

}

#[cfg(not(feature = "bindgen"))]
fn generate_bindings() {

}


#[cfg(not(feature = "legacy"))]
fn set_legacy(_: &mut gcc::Config) {}

#[cfg(feature = "legacy")]
fn set_legacy(config: &mut gcc::Config) {
    config.define("ZSTD_LEGACY_SUPPORT", Some("1"));
}

fn compile_zstd() {
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

fn main() {
    // println!("cargo:rustc-link-lib=zstd");

    compile_zstd();
    generate_bindings();
}
