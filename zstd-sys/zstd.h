#include "zstd/lib/zstd.h"
#include "zstd/lib/dictBuilder/zdict.h"

/* This file is used to generate bindings for both headers.
 * Just run the following command to generate the bindings:

bindgen zstd.h --ctypes-prefix ::libc -- -DZSTD_STATIC_LINKING_ONLY --blacklist-type max_align_t > src/ll.rs

Or use the `bindgen` feature, which will create the bindings automatically.

*/
