#include "rustscript_framework.h"

#include <Arduino.h>
#include <Wire.h>

#include <cstddef>
#include <cstdint>
#include <cstdio>
#include <cstring>

#ifdef RUSTSCRIPT_FEATURE_BLUETOOTH
#include "esp_bt.h"
#endif
#ifdef RUSTSCRIPT_FEATURE_WIFI
#include "esp_event.h"
#include "esp_netif.h"
#include "esp_wifi.h"
#endif
#include "esp_sleep.h"
#include "esp_system.h"
#include "soc/soc_caps.h"

namespace {

constexpr size_t MAX_IO_BYTES = 255;
constexpr size_t PWM_CHANNEL_COUNT = 6;
uint8_t io_buffer[MAX_IO_BYTES];
int8_t pwm_pins[PWM_CHANNEL_COUNT] = {-1, -1, -1, -1, -1, -1};
#ifdef RUSTSCRIPT_FEATURE_WIFI
constexpr size_t NETWORK_TEXT_CAPACITY = 64;
char network_text[NETWORK_TEXT_CAPACITY];
esp_netif_t *wifi_station = nullptr;
bool wifi_initialized = false;
#endif

using host_handler = int32_t (*)(const rustscript_value *, rustscript_value *);

struct host_export {
    const char *name;
    size_t arity;
    host_handler handler;
};

bool is_int(const rustscript_value &value) {
    return value.tag == RUSTSCRIPT_VALUE_INT;
}

bool is_bytes(const rustscript_value &value) {
    return value.tag == RUSTSCRIPT_VALUE_BYTES && (value.len == 0 || value.data != nullptr);
}

#ifdef RUSTSCRIPT_FEATURE_WIFI
bool is_string(const rustscript_value &value) {
    return value.tag == RUSTSCRIPT_VALUE_STRING &&
           (value.len == 0 || value.data != nullptr);
}

bool copy_string(const rustscript_value &value, char *output, size_t capacity) {
    if (!is_string(value) || value.len >= capacity) {
        return false;
    }
    if (value.len != 0) {
        std::memcpy(output, value.data, value.len);
    }
    output[value.len] = '\0';
    return true;
}
#endif

bool valid_pin(int64_t pin) {
    return pin >= 0 && pin < SOC_GPIO_PIN_COUNT;
}

bool valid_i2c_address(int64_t address) {
    return address >= 0x08 && address <= 0x77;
}

int32_t return_bool(rustscript_value *result, bool value) {
    if (result == nullptr) {
        return -1;
    }
    *result = {};
    result->tag = RUSTSCRIPT_VALUE_BOOL;
    result->boolean = value ? 1 : 0;
    return 1;
}

int32_t return_int(rustscript_value *result, int64_t value) {
    if (result == nullptr) {
        return -1;
    }
    *result = {};
    result->tag = RUSTSCRIPT_VALUE_INT;
    result->integer = value;
    return 1;
}

int32_t return_bytes(rustscript_value *result, const uint8_t *data, size_t len) {
    if (result == nullptr || (len != 0 && data == nullptr)) {
        return -1;
    }
    *result = {};
    result->tag = RUSTSCRIPT_VALUE_BYTES;
    result->data = data;
    result->len = len;
    return 1;
}

#ifdef RUSTSCRIPT_FEATURE_WIFI
int32_t return_string(rustscript_value *result, const char *data) {
    if (result == nullptr || data == nullptr) {
        return -1;
    }
    *result = {};
    result->tag = RUSTSCRIPT_VALUE_STRING;
    result->data = reinterpret_cast<const uint8_t *>(data);
    result->len = std::strlen(data);
    return 1;
}
#endif

int32_t gpio_configure(const rustscript_value *args, rustscript_value *result) {
    if (!is_int(args[0]) || !is_int(args[1]) || !valid_pin(args[0].integer)) {
        return -1;
    }
    uint8_t mode;
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
        case 4:
            mode = OUTPUT_OPEN_DRAIN;
            break;
        default:
            return -1;
    }
    pinMode(static_cast<uint8_t>(args[0].integer), mode);
    return return_bool(result, true);
}

int32_t gpio_write(const rustscript_value *args, rustscript_value *result) {
    if (!is_int(args[0]) || args[1].tag != RUSTSCRIPT_VALUE_BOOL ||
        !valid_pin(args[0].integer)) {
        return -1;
    }
    digitalWrite(static_cast<uint8_t>(args[0].integer), args[1].boolean ? HIGH : LOW);
    return return_bool(result, true);
}

int32_t gpio_read(const rustscript_value *args, rustscript_value *result) {
    if (!is_int(args[0]) || !valid_pin(args[0].integer)) {
        return -1;
    }
    return return_bool(
        result,
        digitalRead(static_cast<uint8_t>(args[0].integer)) == HIGH
    );
}

int32_t gpio_analog_read(const rustscript_value *args, rustscript_value *result) {
    if (!is_int(args[0]) || !valid_pin(args[0].integer)) {
        return -1;
    }
    return return_int(result, analogRead(static_cast<uint8_t>(args[0].integer)));
}

int pwm_channel_for_pin(int pin) {
    for (size_t channel = 0; channel < PWM_CHANNEL_COUNT; ++channel) {
        if (pwm_pins[channel] == pin) {
            return static_cast<int>(channel);
        }
    }
    for (size_t channel = 0; channel < PWM_CHANNEL_COUNT; ++channel) {
        if (pwm_pins[channel] < 0) {
            pwm_pins[channel] = static_cast<int8_t>(pin);
            return static_cast<int>(channel);
        }
    }
    return -1;
}

int32_t gpio_pwm(const rustscript_value *args, rustscript_value *result) {
    if (!is_int(args[0]) || !is_int(args[1]) || !is_int(args[2]) || !is_int(args[3]) ||
        !valid_pin(args[0].integer) || args[1].integer < 0 || args[2].integer < 1 ||
        args[2].integer > 40000000 || args[3].integer < 1 || args[3].integer > 16) {
        return -1;
    }
    const uint64_t maximum = (uint64_t{1} << args[3].integer) - 1;
    if (static_cast<uint64_t>(args[1].integer) > maximum) {
        return -1;
    }
    const int channel = pwm_channel_for_pin(static_cast<int>(args[0].integer));
    if (channel < 0 || ledcSetup(channel, args[2].integer, args[3].integer) == 0) {
        return return_bool(result, false);
    }
    ledcAttachPin(static_cast<uint8_t>(args[0].integer), channel);
    ledcWrite(channel, static_cast<uint32_t>(args[1].integer));
    return return_bool(result, true);
}

int32_t i2c_begin(const rustscript_value *args, rustscript_value *result) {
    if (!is_int(args[0]) || !is_int(args[1]) || !is_int(args[2]) ||
        !valid_pin(args[0].integer) || !valid_pin(args[1].integer) ||
        args[2].integer < 1000 || args[2].integer > 5000000) {
        return -1;
    }
    const bool ok = Wire.begin(
        static_cast<int>(args[0].integer),
        static_cast<int>(args[1].integer),
        static_cast<uint32_t>(args[2].integer)
    );
    return return_bool(result, ok);
}

int32_t i2c_end(const rustscript_value *, rustscript_value *) {
    Wire.end();
    return 0;
}

int32_t i2c_write_common(
    const rustscript_value *args,
    rustscript_value *result,
    bool include_register
) {
    const size_t data_index = include_register ? 2 : 1;
    if (!is_int(args[0]) || !valid_i2c_address(args[0].integer) ||
        (include_register && (!is_int(args[1]) || args[1].integer < 0 || args[1].integer > 255)) ||
        !is_bytes(args[data_index]) || args[data_index].len > MAX_IO_BYTES) {
        return -1;
    }
    Wire.beginTransmission(static_cast<uint8_t>(args[0].integer));
    if (include_register) {
        Wire.write(static_cast<uint8_t>(args[1].integer));
    }
    if (args[data_index].len != 0) {
        Wire.write(args[data_index].data, args[data_index].len);
    }
    return return_int(result, Wire.endTransmission());
}

int32_t i2c_write(const rustscript_value *args, rustscript_value *result) {
    return i2c_write_common(args, result, false);
}

int32_t i2c_write_register(const rustscript_value *args, rustscript_value *result) {
    return i2c_write_common(args, result, true);
}

int32_t i2c_read_payload(
    uint8_t address,
    size_t requested,
    rustscript_value *result
) {
    const size_t received = Wire.requestFrom(address, static_cast<uint8_t>(requested));
    size_t count = 0;
    while (Wire.available() && count < received && count < MAX_IO_BYTES) {
        io_buffer[count++] = static_cast<uint8_t>(Wire.read());
    }
    return return_bytes(result, io_buffer, count);
}

int32_t i2c_read(const rustscript_value *args, rustscript_value *result) {
    if (!is_int(args[0]) || !valid_i2c_address(args[0].integer) || !is_int(args[1]) ||
        args[1].integer < 0 || args[1].integer > static_cast<int64_t>(MAX_IO_BYTES)) {
        return -1;
    }
    return i2c_read_payload(
        static_cast<uint8_t>(args[0].integer),
        static_cast<size_t>(args[1].integer),
        result
    );
}

int32_t i2c_read_register(const rustscript_value *args, rustscript_value *result) {
    if (!is_int(args[0]) || !valid_i2c_address(args[0].integer) || !is_int(args[1]) ||
        args[1].integer < 0 || args[1].integer > 255 || !is_int(args[2]) ||
        args[2].integer < 0 || args[2].integer > static_cast<int64_t>(MAX_IO_BYTES)) {
        return -1;
    }
    Wire.beginTransmission(static_cast<uint8_t>(args[0].integer));
    Wire.write(static_cast<uint8_t>(args[1].integer));
    if (Wire.endTransmission(false) != 0) {
        return return_bytes(result, io_buffer, 0);
    }
    return i2c_read_payload(
        static_cast<uint8_t>(args[0].integer),
        static_cast<size_t>(args[2].integer),
        result
    );
}

int32_t mcu_delay_ms(const rustscript_value *args, rustscript_value *) {
    if (!is_int(args[0]) || args[0].integer < 0 || args[0].integer > 60000) {
        return -1;
    }
    delay(static_cast<unsigned long>(args[0].integer));
    return 0;
}

int32_t mcu_delay_us(const rustscript_value *args, rustscript_value *) {
    if (!is_int(args[0]) || args[0].integer < 0 || args[0].integer > 1000000) {
        return -1;
    }
    delayMicroseconds(static_cast<uint32_t>(args[0].integer));
    return 0;
}

int32_t mcu_millis(const rustscript_value *, rustscript_value *result) {
    return return_int(result, millis());
}

int32_t mcu_micros(const rustscript_value *, rustscript_value *result) {
    return return_int(result, micros());
}

int32_t mcu_cpu_frequency(const rustscript_value *, rustscript_value *result) {
    return return_int(result, ESP.getCpuFreqMHz());
}

int32_t mcu_free_heap(const rustscript_value *, rustscript_value *result) {
    return return_int(result, ESP.getFreeHeap());
}

int32_t mcu_flash_size(const rustscript_value *, rustscript_value *result) {
    return return_int(result, ESP.getFlashChipSize());
}

int32_t mcu_random(const rustscript_value *, rustscript_value *result) {
    return return_int(result, esp_random());
}

int32_t mcu_restart(const rustscript_value *, rustscript_value *) {
    Serial.flush();
    ESP.restart();
    return 0;
}

int32_t mcu_deep_sleep(const rustscript_value *args, rustscript_value *) {
    if (!is_int(args[0]) || args[0].integer < 1 || args[0].integer > 86400000000LL) {
        return -1;
    }
    esp_deep_sleep(static_cast<uint64_t>(args[0].integer));
    return 0;
}

#ifdef RUSTSCRIPT_FEATURE_WIFI
esp_err_t initialize_wifi() {
    if (wifi_initialized) {
        return ESP_OK;
    }
    esp_err_t status = esp_netif_init();
    if (status != ESP_OK && status != ESP_ERR_INVALID_STATE) {
        return status;
    }
    status = esp_event_loop_create_default();
    if (status != ESP_OK && status != ESP_ERR_INVALID_STATE) {
        return status;
    }
    wifi_station = esp_netif_create_default_wifi_sta();
    if (wifi_station == nullptr) {
        return ESP_FAIL;
    }
    wifi_init_config_t config = WIFI_INIT_CONFIG_DEFAULT();
    status = esp_wifi_init(&config);
    if (status != ESP_OK) {
        return status;
    }
    wifi_initialized = true;
    return ESP_OK;
}

int32_t wifi_connect(const rustscript_value *args, rustscript_value *result) {
    char ssid[33];
    char password[65];
    if (!copy_string(args[0], ssid, sizeof(ssid)) ||
        !copy_string(args[1], password, sizeof(password)) || ssid[0] == '\0') {
        return -1;
    }
    if (initialize_wifi() != ESP_OK) {
        return return_bool(result, false);
    }
    wifi_config_t config{};
    std::memcpy(config.sta.ssid, ssid, std::strlen(ssid));
    std::memcpy(config.sta.password, password, std::strlen(password));
    config.sta.threshold.authmode = password[0] == '\0' ? WIFI_AUTH_OPEN : WIFI_AUTH_WPA2_PSK;
    config.sta.pmf_cfg.capable = true;
    config.sta.pmf_cfg.required = false;
    esp_err_t status = esp_wifi_set_storage(WIFI_STORAGE_RAM);
    if (status == ESP_OK) {
        status = esp_wifi_set_mode(WIFI_MODE_STA);
    }
    if (status == ESP_OK) {
        status = esp_wifi_set_config(WIFI_IF_STA, &config);
    }
    if (status == ESP_OK) {
        status = esp_wifi_start();
    }
    if (status == ESP_OK) {
        status = esp_wifi_connect();
    }
    return return_bool(result, status == ESP_OK);
}

int32_t wifi_disconnect(const rustscript_value *, rustscript_value *result) {
    return return_bool(result, wifi_initialized && esp_wifi_disconnect() == ESP_OK);
}

int32_t wifi_is_connected(const rustscript_value *, rustscript_value *result) {
    wifi_ap_record_t access_point{};
    return return_bool(
        result,
        wifi_initialized && esp_wifi_sta_get_ap_info(&access_point) == ESP_OK
    );
}

int32_t wifi_rssi(const rustscript_value *, rustscript_value *result) {
    wifi_ap_record_t access_point{};
    if (!wifi_initialized || esp_wifi_sta_get_ap_info(&access_point) != ESP_OK) {
        return return_int(result, -127);
    }
    return return_int(result, access_point.rssi);
}

int32_t wifi_local_ip(const rustscript_value *, rustscript_value *result) {
    esp_netif_ip_info_t info{};
    if (wifi_station == nullptr || esp_netif_get_ip_info(wifi_station, &info) != ESP_OK ||
        info.ip.addr == 0) {
        network_text[0] = '\0';
        return return_string(result, network_text);
    }
    std::snprintf(
        network_text,
        sizeof(network_text),
        IPSTR,
        IP2STR(&info.ip)
    );
    return return_string(result, network_text);
}
#endif

#ifdef RUSTSCRIPT_FEATURE_BLUETOOTH
bool bluetooth_is_active() {
    return esp_bt_controller_get_status() == ESP_BT_CONTROLLER_STATUS_ENABLED;
}

int32_t bluetooth_enable(const rustscript_value *, rustscript_value *result) {
    esp_bt_controller_status_t controller_status = esp_bt_controller_get_status();
    if (controller_status == ESP_BT_CONTROLLER_STATUS_IDLE) {
        esp_bt_controller_config_t config = BT_CONTROLLER_INIT_CONFIG_DEFAULT();
        if (esp_bt_controller_init(&config) != ESP_OK) {
            return return_bool(result, false);
        }
        controller_status = ESP_BT_CONTROLLER_STATUS_INITED;
    }
    if (controller_status == ESP_BT_CONTROLLER_STATUS_INITED &&
        esp_bt_controller_enable(ESP_BT_MODE_BLE) != ESP_OK) {
        return return_bool(result, false);
    }
    return return_bool(result, bluetooth_is_active());
}

int32_t bluetooth_disable(const rustscript_value *, rustscript_value *result) {
    bool ok = true;
    if (esp_bt_controller_get_status() == ESP_BT_CONTROLLER_STATUS_ENABLED) {
        ok = esp_bt_controller_disable() == ESP_OK;
    }
    if (esp_bt_controller_get_status() == ESP_BT_CONTROLLER_STATUS_INITED) {
        ok = esp_bt_controller_deinit() == ESP_OK && ok;
    }
    return return_bool(result, ok);
}

int32_t bluetooth_is_enabled(const rustscript_value *, rustscript_value *result) {
    return return_bool(result, bluetooth_is_active());
}
#endif

int32_t serial_write(const rustscript_value *args, rustscript_value *) {
    if (args[0].tag != RUSTSCRIPT_VALUE_STRING ||
        (args[0].len != 0 && args[0].data == nullptr)) {
        return -1;
    }
    Serial.write(args[0].data, args[0].len);
    Serial.println();
    return 0;
}

int32_t serial_available(const rustscript_value *, rustscript_value *result) {
    return return_int(result, Serial.available());
}

int32_t serial_read(const rustscript_value *args, rustscript_value *result) {
    if (!is_int(args[0]) || args[0].integer < 0 ||
        args[0].integer > static_cast<int64_t>(MAX_IO_BYTES)) {
        return -1;
    }
    size_t count = 0;
    const size_t maximum = static_cast<size_t>(args[0].integer);
    while (count < maximum && Serial.available()) {
        io_buffer[count++] = static_cast<uint8_t>(Serial.read());
    }
    return return_bytes(result, io_buffer, count);
}

constexpr host_export HOST_EXPORTS[] = {
    {"gpio::configure", 2, gpio_configure},
    {"gpio::digital_write", 2, gpio_write},
    {"gpio::digital_read", 1, gpio_read},
    {"gpio::analog_read", 1, gpio_analog_read},
    {"gpio::pwm_write", 4, gpio_pwm},
    {"i2c::open", 3, i2c_begin},
    {"i2c::close", 0, i2c_end},
    {"i2c::transmit", 2, i2c_write},
    {"i2c::transmit_register", 3, i2c_write_register},
    {"i2c::receive", 2, i2c_read},
    {"i2c::receive_register", 3, i2c_read_register},
    {"mcu::delay_ms", 1, mcu_delay_ms},
    {"mcu::delay_us", 1, mcu_delay_us},
    {"mcu::millis", 0, mcu_millis},
    {"mcu::micros", 0, mcu_micros},
    {"mcu::cpu_frequency_mhz", 0, mcu_cpu_frequency},
    {"mcu::free_heap", 0, mcu_free_heap},
    {"mcu::flash_size", 0, mcu_flash_size},
    {"mcu::random", 0, mcu_random},
    {"mcu::restart", 0, mcu_restart},
    {"mcu::deep_sleep_us", 1, mcu_deep_sleep},
#ifdef RUSTSCRIPT_FEATURE_WIFI
    {"wifi::connect", 2, wifi_connect},
    {"wifi::disconnect", 0, wifi_disconnect},
    {"wifi::is_connected", 0, wifi_is_connected},
    {"wifi::rssi", 0, wifi_rssi},
    {"wifi::local_ip", 0, wifi_local_ip},
#endif
#ifdef RUSTSCRIPT_FEATURE_BLUETOOTH
    {"bluetooth::enable", 0, bluetooth_enable},
    {"bluetooth::disable", 0, bluetooth_disable},
    {"bluetooth::is_enabled", 0, bluetooth_is_enabled},
#endif
    {"serial::write_line", 1, serial_write},
    {"serial::available", 0, serial_available},
    {"serial::read_bytes", 1, serial_read},
};

bool host_name_equals(const uint8_t *name, size_t name_len, const char *expected) {
    const size_t expected_len = std::strlen(expected);
    return name_len == expected_len && std::memcmp(name, expected, expected_len) == 0;
}

}  // namespace

int32_t rustscript_dispatch_host(
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
            return entry.handler(args, result);
        }
    }
    return -1;
}
