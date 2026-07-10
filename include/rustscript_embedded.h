#ifndef RUSTSCRIPT_EMBEDDED_H
#define RUSTSCRIPT_EMBEDDED_H

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

enum rustscript_status {
    RUSTSCRIPT_STATUS_OK = 0,
    RUSTSCRIPT_STATUS_INVALID_ARGUMENT = -1,
    RUSTSCRIPT_STATUS_INVALID_VMBC = -2,
    RUSTSCRIPT_STATUS_HOST_ERROR = -3,
    RUSTSCRIPT_STATUS_OUT_OF_FUEL = -4,
    RUSTSCRIPT_STATUS_VM_ERROR = -5,
};

enum rustscript_value_tag {
    RUSTSCRIPT_VALUE_NULL = 0,
    RUSTSCRIPT_VALUE_INT = 1,
    RUSTSCRIPT_VALUE_FLOAT = 2,
    RUSTSCRIPT_VALUE_BOOL = 3,
    RUSTSCRIPT_VALUE_STRING = 4,
    RUSTSCRIPT_VALUE_BYTES = 5,
};

typedef struct rustscript_value {
    uint8_t tag;
    uint8_t boolean;
    uint8_t reserved[6];
    int64_t integer;
    double floating;
    const uint8_t *data;
    size_t len;
} rustscript_value;

typedef int32_t (*rustscript_host_callback)(
    void *context,
    const uint8_t *name,
    size_t name_len,
    const rustscript_value *args,
    size_t arg_count,
    rustscript_value *result
);

int32_t rustscript_run_vmbc(
    const uint8_t *program,
    size_t program_len,
    rustscript_host_callback callback,
    void *context,
    uint64_t fuel
);

void *rustscript_platform_alloc(size_t size, size_t align);
void rustscript_platform_dealloc(void *pointer, size_t size, size_t align);

#ifdef __cplusplus
}
#endif

#endif
