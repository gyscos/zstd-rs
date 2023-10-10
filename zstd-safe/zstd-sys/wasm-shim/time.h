#ifndef _TIME_H
#define _TIME_H

#define CLOCKS_PER_SEC 1000
typedef unsigned long long clock_t;

clock_t rust_zstd_wasm_shim_clock();

inline clock_t clock() {
    return rust_zstd_wasm_shim_clock();
}

#endif // _TIME_H
