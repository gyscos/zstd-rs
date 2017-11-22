#[cfg(feature = "bindgen")]
extern crate bindgen;

extern crate gcc;
extern crate glob;

use std::{env, fs};
use std::path::PathBuf;


#[cfg(feature = "bindgen")]
fn generate_bindings() {
    use std::env;
    use std::path::PathBuf;

    let bindings = bindgen::Builder::default()
        .header("zstd.h")
        .blacklist_type("max_align_t")
        .use_core()
        .ctypes_prefix("::libc")
        .clang_arg("-Izstd/lib")
        .clang_arg("-DZSTD_STATIC_LINKING_ONLY")
        .generate()
        .expect("Unable to generate bindings");

    let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());
    bindings
        .write_to_file(out_path.join("bindings.rs"))
        .expect("Could not write bindings");

}

#[cfg(not(feature = "bindgen"))]
fn generate_bindings() {}


#[cfg(not(feature = "legacy"))]
fn set_legacy(_config: &mut gcc::Build) {}

#[cfg(feature = "legacy")]
fn set_legacy(config: &mut gcc::Build) {
    config.define("ZSTD_LEGACY_SUPPORT", Some("1"));
}

#[cfg(feature = "zstdmt")]
fn set_pthread(config: &mut gcc::Build) {
    config.flag("-pthread");
}

#[cfg(not(feature = "zstdmt"))]
fn set_pthread(_config: &mut gcc::Build) {}

fn compile_zstd() {
    let mut config = gcc::Build::new();

    let globs = &[
        "zstd/lib/common/*.c",
        "zstd/lib/compress/*.c",
        "zstd/lib/decompress/*.c",
        "zstd/lib/legacy/*.c",
        "zstd/lib/deprecated/*.c",
        "zstd/lib/dictBuilder/*.c",
    ];

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
    config.warnings(false);

    set_pthread(&mut config);
    set_legacy(&mut config);

    // Compile!
    config.compile("libzstd.a");

    let src = env::current_dir().unwrap().join("zstd").join("lib");
    let dst = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let include = dst.join("include");
    fs::create_dir_all(&include).unwrap();
    fs::copy(src.join("zstd.h"), include.join("zstd.h")).unwrap();
    let dict_builder = include.join("dictBuilder");
    fs::create_dir_all(&dict_builder).unwrap();
    fs::copy(src.join("dictBuilder").join("zdict.h"), dict_builder.join("zdict.h")).unwrap();
    println!("cargo:root={}", dst.display());
}

fn main() {
    // println!("cargo:rustc-link-lib=zstd");

    compile_zstd();
    generate_bindings();
}
