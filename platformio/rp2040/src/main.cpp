#include <Arduino.h>

#include <cstddef>
#include <cstdint>
#include <cstdlib>
#include <cstring>
#include <limits>

#include "program_vmbc.h"
#include "rustscript_embedded.h"

namespace {

bool host_name_equals(const uint8_t *name, size_t name_len, const char *expected) {
    const size_t expected_len = std::strlen(expected);
    return name_len == expected_len && std::memcmp(name, expected, expected_len) == 0;
}

int32_t dispatch_host(
    void *,
    const uint8_t *name,
    size_t name_len,
    const rustscript_value *args,
    size_t arg_count,
    rustscript_value *
) {
    if (host_name_equals(name, name_len, "gpio_set") && arg_count == 2 &&
        args[0].tag == RUSTSCRIPT_VALUE_INT && args[1].tag == RUSTSCRIPT_VALUE_BOOL) {
        digitalWrite(static_cast<uint8_t>(args[0].integer), args[1].boolean ? HIGH : LOW);
        return 0;
    }
    if (host_name_equals(name, name_len, "delay_ms") && arg_count == 1 &&
        args[0].tag == RUSTSCRIPT_VALUE_INT && args[0].integer >= 0) {
        delay(static_cast<unsigned long>(args[0].integer));
        return 0;
    }
    if (host_name_equals(name, name_len, "serial_write") && arg_count == 1 &&
        args[0].tag == RUSTSCRIPT_VALUE_STRING) {
        Serial.write(args[0].data, args[0].len);
        Serial.println();
        return 0;
    }
    return -1;
}

}  // namespace

extern "C" void *rustscript_platform_alloc(size_t size, size_t align) {
    if (size == 0 || align == 0 || (align & (align - 1)) != 0) {
        return nullptr;
    }
    if (align <= alignof(std::max_align_t)) {
        return std::malloc(size);
    }
    if (size > std::numeric_limits<size_t>::max() - align - sizeof(void *)) {
        return nullptr;
    }
    void *raw = std::malloc(size + align - 1 + sizeof(void *));
    if (raw == nullptr) {
        return nullptr;
    }
    const uintptr_t start = reinterpret_cast<uintptr_t>(raw) + sizeof(void *);
    const uintptr_t aligned = (start + align - 1) & ~(static_cast<uintptr_t>(align) - 1);
    reinterpret_cast<void **>(aligned)[-1] = raw;
    return reinterpret_cast<void *>(aligned);
}

extern "C" void rustscript_platform_dealloc(void *pointer, size_t, size_t align) {
    if (pointer == nullptr) {
        return;
    }
    if (align <= alignof(std::max_align_t)) {
        std::free(pointer);
        return;
    }
    std::free(reinterpret_cast<void **>(pointer)[-1]);
}

void setup() {
    Serial.begin(115200);
    pinMode(LED_BUILTIN, OUTPUT);
    const int32_t status = rustscript_run_vmbc(
        RUSTSCRIPT_PROGRAM_VMBC,
        RUSTSCRIPT_PROGRAM_VMBC_LEN,
        dispatch_host,
        nullptr,
        100000
    );
    Serial.print("rustscript:status=");
    Serial.println(status);
}

void loop() {
    delay(1000);
}
