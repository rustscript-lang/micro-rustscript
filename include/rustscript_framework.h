#pragma once

#include <cstddef>
#include <cstdint>

#include "rustscript_embedded.h"

int32_t rustscript_dispatch_host(
    void *context,
    const uint8_t *name,
    size_t name_len,
    const rustscript_value *args,
    size_t arg_count,
    rustscript_value *result
);
