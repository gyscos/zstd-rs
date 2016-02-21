extern crate gcc;

fn main() {
    gcc::compile_library("libzstd.a",
                         &["zstd/lib/zstd_compress.c",
                           "zstd/lib/zstd_decompress.c",
                           "zstd/lib/fse.c",
                           "zstd/lib/huff0.c",
                           "zstd/lib/zdict.c",
                           "zstd/lib/divsufsort.c",
                           "zstd/lib/zbuff.c"]);
}
