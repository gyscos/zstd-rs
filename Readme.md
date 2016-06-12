# zstd

[![Build Status](https://travis-ci.org/gyscos/zstd-rs.svg?branch=master)](https://travis-ci.org/gyscos/zstd-rs)
[![crates.io](http://meritbadge.herokuapp.com/zstd)](https://crates.io/crates/zstd)
[![MIT licensed](https://img.shields.io/badge/license-MIT-blue.svg)](./LICENSE)

This library is a rust binding for the [zstd compression library][zstd].

# [Documentation][doc]

## 1 - Add to `cargo.toml`

### Using [cargo-edit]

```bash
$ cargo add zstd
```

### Manually

```toml
# Cargo.toml

[dependencies]
zstd = "0.1"
```

## 2 - Usage

Check the [stream] example:

```rust
extern crate zstd;

use std::io;

fn compress(level: i32) {
	let mut encoder = zstd::Encoder::new(io::stdout(), level).unwrap();
	io::copy(&mut io::stdin(), &mut encoder).unwrap();
    encoder.finish().unwrap();
}

fn decompress() {
	let mut decoder = zstd::Decoder::new(io::stdin()).unwrap();
	io::copy(&mut decoder, &mut io::stdout()).unwrap();
}
```

# Compile it yourself

This repository includes `zstd` as a [submodule]. To get everything during your clone, use :

```
git clone https://github.com/gyscos/zstd-rs --recursive
```

Or, if you cloned it without the `--recursive` flag, call this from inside the repository:

```
git submodule update --init
```

Then, running `cargo build` should take care of building the C library and linking to it.

# TODO

* Benchmarks, optimizations, ...

# Disclaimer

This implementation is largely inspired by bozaro's [lz4-rs].

# License

[MIT](LICENSE)

[zstd]: https://github.com/Cyan4973/zstd
[lz4-rs]: https://github.com/bozaro/lz4-rs
[cargo-edit]: https://github.com/killercup/cargo-edit#cargo-add
[doc]: https://gyscos.github.io/zstd-rs/zstd/index.html
[stream]: examples/stream.rs
[submodule]: https://git-scm.com/book/en/v2/Git-Tools-Submodules
