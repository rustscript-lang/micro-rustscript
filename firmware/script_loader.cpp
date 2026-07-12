#include "rustscript_loader.h"

#include <Arduino.h>
#include <SD.h>

#include <cstddef>
#include <cstdint>
#include <cstdlib>
#include <cstring>

#include "esp_partition.h"
#include "rustscript_embedded.h"
#include "rustscript_framework.h"

#ifndef RUSTSCRIPT_SD_CS
#define RUSTSCRIPT_SD_CS 7
#endif

#ifndef RUSTSCRIPT_SD_PATH
#define RUSTSCRIPT_SD_PATH "/rustscript/main.vmbc"
#endif

namespace {

constexpr char SCRIPT_PARTITION_LABEL[] = "rustscript";
constexpr uint8_t SCRIPT_PARTITION_TYPE = 0x40;
constexpr uint32_t SCRIPT_IMAGE_VERSION = 1;
constexpr uint8_t SCRIPT_IMAGE_MAGIC[8] = {'R', 'S', 'S', 'V', 'M', 'B', 'C', 0};
constexpr size_t MAX_REPL_LINE = 128;

struct __attribute__((packed)) ScriptImageHeader {
    uint8_t magic[8];
    uint32_t version;
    uint32_t payload_len;
    uint32_t crc32;
};

const esp_partition_t *script_partition() {
    return esp_partition_find_first(
        static_cast<esp_partition_type_t>(SCRIPT_PARTITION_TYPE),
        ESP_PARTITION_SUBTYPE_ANY,
        SCRIPT_PARTITION_LABEL
    );
}

bool header_valid(const ScriptImageHeader &header, size_t partition_size) {
    return std::memcmp(header.magic, SCRIPT_IMAGE_MAGIC, sizeof(SCRIPT_IMAGE_MAGIC)) == 0 &&
           header.version == SCRIPT_IMAGE_VERSION && header.payload_len != 0 &&
           header.payload_len <= partition_size - sizeof(ScriptImageHeader);
}

bool read_serial_payload(uint8_t *data, size_t len) {
    size_t offset = 0;
    const unsigned long previous_timeout = Serial.getTimeout();
    Serial.setTimeout(10000);
    while (offset < len) {
        const size_t count = Serial.readBytes(data + offset, len - offset);
        if (count == 0) {
            Serial.setTimeout(previous_timeout);
            return false;
        }
        offset += count;
    }
    Serial.setTimeout(previous_timeout);
    return true;
}

void run_repl_payload(const uint8_t *data, size_t len) {
    const int32_t status = rustscript_run_vmbc(
        data,
        len,
        rustscript_dispatch_host,
        nullptr,
        1000000
    );
    Serial.print("rss:status=");
    Serial.println(status);
}

void print_repl_help() {
    Serial.println("rss:VMBC REPL ready");
    Serial.println("rss:load <length> <crc32-hex>   run one VMBC payload");
    Serial.println("rss:install <length> <crc32-hex> write and run the script partition");
    Serial.println("rss:run                          run the script partition");
    Serial.println("rss:info                         show partition information");
    Serial.println("rss:help");
}

bool parse_transfer_command(
    const String &line,
    const char *command,
    size_t *length,
    uint32_t *expected_crc
) {
    const size_t prefix_len = std::strlen(command);
    if (!line.startsWith(command) || line.length() <= prefix_len || line[prefix_len] != ' ') {
        return false;
    }
    const char *input = line.c_str() + prefix_len + 1;
    char *end = nullptr;
    const unsigned long parsed_len = std::strtoul(input, &end, 10);
    if (end == input || *end != ' ' || parsed_len == 0) {
        return false;
    }
    input = end + 1;
    const unsigned long parsed_crc = std::strtoul(input, &end, 16);
    if (end == input || *end != '\0') {
        return false;
    }
    *length = static_cast<size_t>(parsed_len);
    *expected_crc = static_cast<uint32_t>(parsed_crc);
    return true;
}

void receive_repl_payload(size_t len, uint32_t expected_crc, bool install) {
    const esp_partition_t *partition = script_partition();
    const size_t maximum = partition == nullptr
                               ? 0
                               : partition->size - sizeof(ScriptImageHeader);
    if (len > maximum) {
        Serial.println("rss:error=payload-too-large");
        return;
    }
    auto *payload = static_cast<uint8_t *>(std::malloc(len));
    if (payload == nullptr) {
        Serial.println("rss:error=allocation-failed");
        return;
    }
    Serial.println("rss:ready");
    if (!read_serial_payload(payload, len)) {
        Serial.println("rss:error=transfer-timeout");
        std::free(payload);
        return;
    }
    const uint32_t actual_crc = rustscript_crc32(payload, len);
    if (actual_crc != expected_crc) {
        Serial.print("rss:error=crc-mismatch actual=");
        Serial.println(actual_crc, HEX);
        std::free(payload);
        return;
    }
    if (install && !rustscript_install_partition(payload, len)) {
        Serial.println("rss:error=partition-write-failed");
        std::free(payload);
        return;
    }
    Serial.println(install ? "rss:installed" : "rss:loaded");
    run_repl_payload(payload, len);
    std::free(payload);
}

}  // namespace

uint32_t rustscript_crc32(const uint8_t *data, size_t len) {
    uint32_t crc = 0xffffffffU;
    for (size_t index = 0; index < len; ++index) {
        crc ^= data[index];
        for (uint8_t bit = 0; bit < 8; ++bit) {
            const uint32_t mask = 0U - (crc & 1U);
            crc = (crc >> 1U) ^ (0xedb88320U & mask);
        }
    }
    return ~crc;
}

bool rustscript_load_sd(RustScriptImage *image) {
    if (image == nullptr || !SD.begin(RUSTSCRIPT_SD_CS)) {
        return false;
    }
    File file = SD.open(RUSTSCRIPT_SD_PATH, FILE_READ);
    if (!file || file.isDirectory() || file.size() == 0) {
        if (file) {
            file.close();
        }
        SD.end();
        return false;
    }
    const size_t len = static_cast<size_t>(file.size());
    auto *data = static_cast<uint8_t *>(std::malloc(len));
    if (data == nullptr) {
        file.close();
        SD.end();
        return false;
    }
    const size_t count = file.read(data, len);
    file.close();
    SD.end();
    if (count != len) {
        std::free(data);
        return false;
    }
    image->data = data;
    image->len = len;
    image->source = "sd:/rustscript/main.vmbc";
    return true;
}

bool rustscript_load_partition(RustScriptImage *image) {
    if (image == nullptr) {
        return false;
    }
    const esp_partition_t *partition = script_partition();
    if (partition == nullptr || partition->size <= sizeof(ScriptImageHeader)) {
        return false;
    }
    ScriptImageHeader header{};
    if (esp_partition_read(partition, 0, &header, sizeof(header)) != ESP_OK ||
        !header_valid(header, partition->size)) {
        return false;
    }
    auto *data = static_cast<uint8_t *>(std::malloc(header.payload_len));
    if (data == nullptr) {
        return false;
    }
    if (esp_partition_read(partition, sizeof(header), data, header.payload_len) != ESP_OK ||
        rustscript_crc32(data, header.payload_len) != header.crc32) {
        std::free(data);
        return false;
    }
    image->data = data;
    image->len = header.payload_len;
    image->source = "flash:rustscript";
    return true;
}

bool rustscript_install_partition(const uint8_t *data, size_t len) {
    const esp_partition_t *partition = script_partition();
    if (partition == nullptr || data == nullptr || len == 0 ||
        len > partition->size - sizeof(ScriptImageHeader)) {
        return false;
    }
    ScriptImageHeader header{};
    std::memcpy(header.magic, SCRIPT_IMAGE_MAGIC, sizeof(SCRIPT_IMAGE_MAGIC));
    header.version = SCRIPT_IMAGE_VERSION;
    header.payload_len = static_cast<uint32_t>(len);
    header.crc32 = rustscript_crc32(data, len);
    return esp_partition_erase_range(partition, 0, partition->size) == ESP_OK &&
           esp_partition_write(partition, 0, &header, sizeof(header)) == ESP_OK &&
           esp_partition_write(partition, sizeof(header), data, len) == ESP_OK;
}

void rustscript_free_image(RustScriptImage *image) {
    if (image == nullptr) {
        return;
    }
    std::free(image->data);
    image->data = nullptr;
    image->len = 0;
    image->source = nullptr;
}

void rustscript_repl_source();
void rustscript_repl() {
    rustscript_repl_source();
}

uint8_t *read_serial_binary(size_t len) {
    if (len == 0) {
        return nullptr;
    }
    auto *buffer = static_cast<uint8_t *>(std::malloc(len));
    if (buffer == nullptr) {
        return nullptr;
    }
    if (!read_serial_payload(buffer, len)) {
        Serial.println("rss:error=payload-timeout");
        std::free(buffer);
        return nullptr;
    }
    return buffer;
}

void rustscript_repl_source() {
    print_repl_help();
    Serial.println("rss:pd-vm> ");
    while (true) {
        if (!Serial.available()) {
            delay(10);
            continue;
        }
        if (Serial.peek() == 'R') {
            uint8_t frame_header[REPL_FRAME_HEADER_SIZE] = {};
            if (!read_serial_payload(frame_header, sizeof(frame_header))) {
                continue;
            }
            if (std::memcmp(frame_header, REPL_REQUEST_MAGIC, 4) != 0) {
                Serial.println("rss:error=invalid-repl-magic");
                continue;
            }
            uint32_t program_len = 0;
            uint32_t state_len = 0;
            std::memcpy(&program_len, frame_header + 4, sizeof(program_len));
            std::memcpy(&state_len, frame_header + 8, sizeof(state_len));
            if (program_len == 0 || state_len == 0 ||
                program_len > REPL_MAX_FRAME_SIZE || state_len > REPL_MAX_FRAME_SIZE ||
                program_len > REPL_MAX_FRAME_SIZE - state_len) {
                Serial.println("rss:error=invalid-repl-length");
                continue;
            }
            uint8_t *program = read_serial_binary(program_len);
            if (program == nullptr) {
                continue;
            }
            uint8_t *state = read_serial_binary(state_len);
            if (state == nullptr) {
                std::free(program);
                continue;
            }
            rustscript_buffer output = {};
            const int32_t status = rustscript_repl_run_vmbc(
                program, program_len,
                state, state_len,
                rustscript_dispatch_host, nullptr, 1000000,
                &output
            );
            std::free(program);
            std::free(state);

            uint8_t response_header[REPL_FRAME_HEADER_SIZE] = {};
            std::memcpy(response_header, REPL_RESPONSE_MAGIC, 4);
            const uint32_t output_len = static_cast<uint32_t>(output.len);
            std::memcpy(response_header + 4, &status, sizeof(status));
            std::memcpy(response_header + 8, &output_len, sizeof(output_len));
            Serial.write(response_header, sizeof(response_header));
            if (output.len > 0) {
                Serial.write(output.data, output.len);
                rustscript_buffer_free(output);
            }
            Serial.flush();
            continue;
        }

        String header = Serial.readStringUntil('\n');
        header.trim();
        if (header.length() == 0) {
            continue;
        }
        // Legacy commands
        size_t len = 0;
        uint32_t crc = 0;
        if (parse_transfer_command(header, "load", &len, &crc)) {
            receive_repl_payload(len, crc, false);
        } else if (parse_transfer_command(header, "install", &len, &crc)) {
            receive_repl_payload(len, crc, true);
        } else if (header == "run") {
            RustScriptImage image{};
            if (rustscript_load_partition(&image)) {
                run_repl_payload(image.data, image.len);
                rustscript_free_image(&image);
            } else {
                Serial.println("rss:error=no-partition-script");
            }
        } else if (header == "info") {
            const esp_partition_t *partition = script_partition();
            if (partition == nullptr) {
                Serial.println("rss:partition=missing");
            } else {
                Serial.printf(
                    "rss:partition offset=0x%lx size=%lu\n",
                    static_cast<unsigned long>(partition->address),
                    static_cast<unsigned long>(partition->size)
                );
            }
        } else if (header == "help") {
            print_repl_help();
        } else {
            Serial.println("rss:error=unknown-command");
        }
    }
}

