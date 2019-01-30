#ifdef PKG_CONFIG

/* Just use installed headers */
#include <zstd.h>
#include <zdict.h>
// Don't use experimental features like zstdmt

#else // #ifdef PKG_CONFIG

#include "zstd/lib/zstd.h"
#include "zstd/lib/dictBuilder/zdict.h"
#ifdef ZSTD_STATIC_LINKING_ONLY
#include "zstd/lib/compress/zstdmt_compress.h"
#endif // #ifdef ZSTD_STATIC_LINKING_ONLY

#endif // #ifdef PKG_CONFIG


/* This file is used to generate bindings for both headers.
 * Check update_bindings.sh to see how to use it.
 * Or use the `bindgen` feature, which will create the bindings automatically. */
