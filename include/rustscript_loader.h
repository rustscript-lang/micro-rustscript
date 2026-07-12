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
#ifdef __cplusplus
extern "C" {
#endif
void rustscript_repl();
#ifdef __cplusplus
}
#endif

#define REPL_REQUEST_MAGIC "RSSQ"
#define REPL_RESPONSE_MAGIC "RSSP"
#define REPL_FRAME_HEADER_SIZE 12
#define REPL_MAX_FRAME_SIZE (16U * 1024U * 1024U)

// Request:  "RSSQ" + program_len(u32 LE) + state_len(u32 LE) + program + state.
// Response: "RSSP" + status(i32 LE) + response_len(u32 LE) + response.
