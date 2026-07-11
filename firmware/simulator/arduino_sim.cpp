#include <Arduino.h>

#include <array>
#include <cstdio>

namespace {
std::array<uint8_t, 64> pin_levels{};
}

SimSerial Serial;

extern "C" void rust_eh_personality() {}

void SimSerial::begin(unsigned long baud) {
    std::printf("sim:serial=%lu\n", baud);
}

void SimSerial::print(const char *value) {
    std::fputs(value, stdout);
}

void SimSerial::print(int32_t value) {
    std::printf("%d", value);
}

void SimSerial::println() {
    std::putchar('\n');
}

void SimSerial::println(int32_t value) {
    std::printf("%d\n", value);
}

size_t SimSerial::write(const uint8_t *data, size_t len) {
    return std::fwrite(data, 1, len, stdout);
}

void pinMode(uint8_t pin, uint8_t mode) {
    std::printf("sim:pin-mode=%u,%u\n", pin, mode);
}

void digitalWrite(uint8_t pin, uint8_t value) {
    if (pin < pin_levels.size()) {
        pin_levels[pin] = value;
    }
    std::printf("sim:digital-write=%u,%u\n", pin, value);
}

int digitalRead(uint8_t pin) {
    return pin < pin_levels.size() ? pin_levels[pin] : LOW;
}

void delay(unsigned long milliseconds) {
    std::printf("sim:delay-ms=%lu\n", milliseconds);
}

int main() {
    setup();
    return 0;
}