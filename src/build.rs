extern crate gcc;

fn main() {
    let mut config = gcc::Config::new();

    let source_files = &["zstd/lib/compress/zstd_compress.c",
                         "zstd/lib/compress/fse_compress.c",
                         "zstd/lib/compress/huf_compress.c",
                         "zstd/lib/compress/zbuff_compress.c",

                         "zstd/lib/decompress/zstd_decompress.c",
                         "zstd/lib/common/fse_decompress.c",
                         "zstd/lib/decompress/huf_decompress.c",
                         "zstd/lib/decompress/zbuff_decompress.c",

                         "zstd/lib/dictBuilder/zdict.c",
                         "zstd/lib/dictBuilder/divsufsort.c",

                         "zstd/lib/common/zstd_common.c",
                         "zstd/lib/common/entropy_common.c"];
    for file in source_files {
        config.file(file);
    }

    // Some extra parameters
    config.opt_level(3);
    config.include("zstd/lib/common");

    // Compile!
    config.compile("libzstd.a");
}
