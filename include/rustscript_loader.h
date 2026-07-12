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

#define REPL_MAGIC "RSSR"
#define REPL_LEN_SIZE 4
#define REPL_CMD_HELLO  1
#define REPL_CMD_HELLO_RESPONSE 2
#define REPL_CMD_PAYLOAD 3
#define REPL_CMD_PAYLOAD_RESPONSE 4
#define REPL_CMD_ERROR  5

// Power-on state: send HELLO -> receive HELLO_RESPONSE (or timeout -> legacy PROMPT).
// Main loop: send PAYLOAD (program_len + state_len + program + state)
//        -> receive PAYLOAD_RESPONSE (status_code + response_len + response)
//        or ERROR (message_len + message).
