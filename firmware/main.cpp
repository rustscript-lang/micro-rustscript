#include <Arduino.h>

#include <cstddef>
#include <cstdint>
#include <cstdlib>
#include <limits>

#include "rustscript_embedded.h"
#include "rustscript_framework.h"
#include "rustscript_loader.h"

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

    RustScriptImage image{};
    if (!rustscript_load_sd(&image)) {
        rustscript_load_partition(&image);
    }
    if (image.data == nullptr) {
        Serial.println("rss:boot=no-script");
        rustscript_repl();
        return;
    }

    Serial.print("rss:boot=");
    Serial.println(image.source);
    const int32_t status = rustscript_run_vmbc(
        image.data,
        image.len,
        rustscript_dispatch_host,
        nullptr,
        1000000
    );
    rustscript_free_image(&image);
    Serial.print("rss:status=");
    Serial.println(status);
    rustscript_repl();
}

void loop() {
    delay(1000);
}
