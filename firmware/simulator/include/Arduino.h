#pragma once

#include <cstddef>
#include <cstdint>

#define HIGH 1
#define LOW 0
#define INPUT 0
#define OUTPUT 1

class SimSerial {
public:
    void begin(unsigned long baud);
    void print(const char *value);
    void print(int32_t value);
    void println();
    void println(int32_t value);
    size_t write(const uint8_t *data, size_t len);
};

extern SimSerial Serial;

void pinMode(uint8_t pin, uint8_t mode);
void digitalWrite(uint8_t pin, uint8_t value);
int digitalRead(uint8_t pin);
void delay(unsigned long milliseconds);

void setup();
void loop();