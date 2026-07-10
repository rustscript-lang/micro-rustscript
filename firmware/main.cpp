#include <Arduino.h>

#include <cstddef>
#include <cstdint>
#include <cstdlib>
#include <cstring>
#include <limits>

#include "program_vmbc.h"
#include "rustscript_embedded.h"
#include "soc/soc_caps.h"

namespace {

using host_handler = int32_t (*)(const rustscript_value *, size_t, rustscript_value *);

struct host_export {
    const char *name;
    size_t arity;
    host_handler handler;
};

bool value_is_int(const rustscript_value &value) {
    return value.tag == RUSTSCRIPT_VALUE_INT;
}

bool valid_pin(int64_t pin) {
    return pin >= 0 && pin < SOC_GPIO_PIN_COUNT;
}

int32_t host_gpio_mode(const rustscript_value *args, size_t, rustscript_value *) {
    if (!value_is_int(args[0]) || !value_is_int(args[1]) || !valid_pin(args[0].integer)) {
        return -1;
    }
    uint8_t mode = INPUT;
    switch (args[1].integer) {
        case 0:
            mode = INPUT;
            break;
        case 1:
            mode = OUTPUT;
            break;
        case 2:
            mode = INPUT_PULLUP;
            break;
        case 3:
            mode = INPUT_PULLDOWN;
            break;
        default:
            return -1;
    }
    pinMode(static_cast<uint8_t>(args[0].integer), mode);
    return 0;
}

int32_t host_gpio_write(const rustscript_value *args, size_t, rustscript_value *) {
    if (!value_is_int(args[0]) || args[1].tag != RUSTSCRIPT_VALUE_BOOL ||
        !valid_pin(args[0].integer)) {
        return -1;
    }
    digitalWrite(static_cast<uint8_t>(args[0].integer), args[1].boolean ? HIGH : LOW);
    return 0;
}

int32_t host_gpio_read(const rustscript_value *args, size_t, rustscript_value *result) {
    if (!value_is_int(args[0]) || !valid_pin(args[0].integer) || result == nullptr) {
        return -1;
    }
    *result = {};
    result->tag = RUSTSCRIPT_VALUE_BOOL;
    result->boolean = digitalRead(static_cast<uint8_t>(args[0].integer)) == HIGH;
    return 1;
}

int32_t host_delay_ms(const rustscript_value *args, size_t, rustscript_value *) {
    if (!value_is_int(args[0]) || args[0].integer < 0 || args[0].integer > 60000) {
        return -1;
    }
    delay(static_cast<unsigned long>(args[0].integer));
    return 0;
}

int32_t host_serial_write(const rustscript_value *args, size_t, rustscript_value *) {
    if (args[0].tag != RUSTSCRIPT_VALUE_STRING ||
        (args[0].len != 0 && args[0].data == nullptr)) {
        return -1;
    }
    Serial.write(args[0].data, args[0].len);
    Serial.println();
    return 0;
}

constexpr host_export HOST_EXPORTS[] = {
    {"gpio_mode", 2, host_gpio_mode},
    {"gpio_write", 2, host_gpio_write},
    {"gpio_read", 1, host_gpio_read},
    {"delay_ms", 1, host_delay_ms},
    {"serial_write", 1, host_serial_write},
};

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
    rustscript_value *result
) {
    if ((name_len != 0 && name == nullptr) || (arg_count != 0 && args == nullptr)) {
        return -1;
    }
    for (const auto &entry : HOST_EXPORTS) {
        if (entry.arity == arg_count && host_name_equals(name, name_len, entry.name)) {
            return entry.handler(args, arg_count, result);
        }
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
    delay(250);
    const int32_t status = rustscript_run_vmbc(
        RUSTSCRIPT_PROGRAM_VMBC,
        RUSTSCRIPT_PROGRAM_VMBC_LEN,
        dispatch_host,
        nullptr,
        100000
    );
    Serial.print("micro-rustscript:status=");
    Serial.println(status);
}

void loop() {
    delay(1000);
}
