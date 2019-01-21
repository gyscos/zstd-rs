#ifdef PKG_CONFIG
/* Just use installed headers */
#include <zstd.h>
#include <zdict.h>
#else
#include "zstd/lib/zstd.h"
#include "zstd/lib/dictBuilder/zdict.h"
#include "zstd/lib/compress/zstdmt_compress.h"
#endif

/* This file is used to generate bindings for both headers.
 * Just run the following command to generate the bindings:

bindgen zstd.h --ctypes-prefix ::libc --blacklist-type max_align_t --rustified-enum '.*' --use-core -o src/bindings.rs -- -DZSTD_STATIC_LINKING_ONLY

Or use the `bindgen` feature, which will create the bindings automatically.

*/
