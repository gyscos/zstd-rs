#[cfg(feature = "bindgen")]
extern crate bindgen;

#[cfg(feature = "pkg-config")]
extern crate pkg_config;

extern crate cc;
extern crate glob;
extern crate itertools;

use std::path::PathBuf;
use std::{env, fs};

use itertools::Itertools;

#[cfg(feature = "bindgen")]
fn generate_bindings(defs: Vec<&str>, headerpaths: Vec<PathBuf>) {
    let bindings = bindgen::Builder::default()
        .header("zstd.h")
        .blacklist_type("max_align_t")
        .size_t_is_usize(true)
        .use_core()
        .rustified_enum(".*")
        .clang_args(
            headerpaths
                .into_iter()
                .map(|path| format!("-I{}", path.display())),
        )
        .clang_args(defs.into_iter().map(|def| format!("-D{}", def)));

    #[cfg(feature = "experimental")]
    let bindings = bindings.clang_arg("-DZSTD_STATIC_LINKING_ONLY");

    #[cfg(not(feature = "std"))]
    let bindings = bindings.ctypes_prefix("libc");

    let bindings = bindings.generate().expect("Unable to generate bindings");

    let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());
    bindings
        .write_to_file(out_path.join("bindings.rs"))
        .expect("Could not write bindings");
}

#[cfg(not(feature = "bindgen"))]
fn generate_bindings(_: Vec<&str>, _: Vec<PathBuf>) {}

#[cfg(feature = "pkg-config")]
fn pkg_config() -> (Vec<&'static str>, Vec<PathBuf>) {
    let library = pkg_config::Config::new()
        .statik(true)
        .cargo_metadata(!cfg!(feature = "non-cargo"))
        .probe("libzstd")
        .expect("Can't probe for zstd in pkg-config");
    (vec!["PKG_CONFIG"], library.include_paths)
}

#[cfg(not(feature = "pkg-config"))]
fn pkg_config() -> (Vec<&'static str>, Vec<PathBuf>) {
    unimplemented!()
}

#[cfg(not(feature = "legacy"))]
fn set_legacy(_config: &mut cc::Build) {}

#[cfg(feature = "legacy")]
fn set_legacy(config: &mut cc::Build) {
    config.define("ZSTD_LEGACY_SUPPORT", Some("1"));
}

#[cfg(feature = "zstdmt")]
fn set_pthread(config: &mut cc::Build) {
    config.flag("-pthread");
}

#[cfg(not(feature = "zstdmt"))]
fn set_pthread(_config: &mut cc::Build) {}

#[cfg(feature = "zstdmt")]
fn enable_threading(config: &mut cc::Build) {
    config.define("ZSTD_MULTITHREAD", Some(""));
}

#[cfg(not(feature = "zstdmt"))]
fn enable_threading(_config: &mut cc::Build) {}

fn compile_zstd() {
    let mut config = cc::Build::new();

    let globs = &[
        "zstd/lib/common/*.c",
        "zstd/lib/compress/*.c",
        "zstd/lib/decompress/*.c",
        "zstd/lib/legacy/*.c",
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

    config.define("ZSTD_LIB_DEPRECATED", Some("0"));

    // Hide symbols from resulting library,
    // so we can be used with another zstd-linking lib.
    // See https://github.com/gyscos/zstd-rs/issues/58
    config.flag("-fvisibility=hidden");
    config.define("ZSTDLIB_VISIBILITY", Some(""));
    config.define("ZDICTLIB_VISIBILITY", Some(""));
    config.define("ZSTDERRORLIB_VISIBILITY", Some(""));

    set_pthread(&mut config);
    set_legacy(&mut config);
    enable_threading(&mut config);

    // Compile!
    config.compile("libzstd.a");

    let src = env::current_dir().unwrap().join("zstd").join("lib");
    let dst = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let include = dst.join("include");
    fs::create_dir_all(&include).unwrap();
    fs::copy(src.join("zstd.h"), include.join("zstd.h")).unwrap();
    fs::copy(
        src.join("common").join("zstd_errors.h"),
        include.join("zstd_errors.h"),
    )
    .unwrap();
    fs::copy(
        src.join("dictBuilder").join("zdict.h"),
        include.join("zdict.h"),
    )
    .unwrap();
    println!("cargo:root={}", dst.display());
}

fn main() {
    // println!("cargo:rustc-link-lib=zstd");
    let (defs, headerpaths) = if cfg!(feature = "pkg-config") {
        pkg_config()
    } else {
        if !PathBuf::from("zstd/lib").exists() {
            panic!("Folder 'zstd/lib' does not exists. Maybe you forget clone 'zstd' submodule?");
        }

        let manifest_dir = PathBuf::from(
            env::var("CARGO_MANIFEST_DIR")
                .expect("Manifest dir is always set by cargo"),
        );

        compile_zstd();
        (vec![], vec![manifest_dir.join("zstd/lib")])
    };

    println!("cargo:include={}", headerpaths.iter().map(|p| p.display().to_string()).join(";"));

    generate_bindings(defs, headerpaths);
}
