#pragma once

#include <cstddef>
#include <cstdint>

struct RustScriptImage {
    uint8_t *data;
    size_t len;
    const char *source;
};

bool rustscript_load_sd(RustScriptImage *image);
bool rustscript_load_partition(RustScriptImage *image);
bool rustscript_install_partition(const uint8_t *data, size_t len);
void rustscript_free_image(RustScriptImage *image);
uint32_t rustscript_crc32(const uint8_t *data, size_t len);
void rustscript_repl();
