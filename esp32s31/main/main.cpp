#include <cstddef>
#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <cstring>

#include "driver/gpio.h"
#ifdef RUSTSCRIPT_FEATURE_BLUETOOTH
#include "esp_bt.h"
#endif
#ifdef RUSTSCRIPT_FEATURE_WIFI
#include "esp_event.h"
#include "esp_netif.h"
#include "esp_wifi.h"
#endif
#include "esp_heap_caps.h"
#include "esp_random.h"
#include "esp_system.h"
#include "esp_timer.h"
#include "freertos/FreeRTOS.h"
#include "freertos/task.h"
#include "program_vmbc.h"
#include "rustscript_embedded.h"

namespace {

#ifdef RUSTSCRIPT_FEATURE_WIFI
char network_text[64];
esp_netif_t *wifi_station = nullptr;
bool wifi_initialized = false;
#endif

bool name_equals(const uint8_t *name, size_t name_len, const char *expected) {
    const size_t expected_len = std::strlen(expected);
    return name_len == expected_len && std::memcmp(name, expected, expected_len) == 0;
}

bool is_int(const rustscript_value &value) {
    return value.tag == RUSTSCRIPT_VALUE_INT;
}

bool is_gpio(const rustscript_value &value) {
    return is_int(value) && value.integer >= 0 && value.integer < GPIO_NUM_MAX;
}

bool is_string(const rustscript_value &value) {
    return value.tag == RUSTSCRIPT_VALUE_STRING &&
           (value.len == 0 || value.data != nullptr);
}

#ifdef RUSTSCRIPT_FEATURE_WIFI
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

#ifdef RUSTSCRIPT_FEATURE_WIFI
int32_t return_string(rustscript_value *result, const char *value) {
    if (result == nullptr || value == nullptr) {
        return -1;
    }
    *result = {};
    result->tag = RUSTSCRIPT_VALUE_STRING;
    result->data = reinterpret_cast<const uint8_t *>(value);
    result->len = std::strlen(value);
    return 1;
}

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
    if (status == ESP_OK) {
        wifi_initialized = true;
    }
    return status;
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
    if (status == ESP_OK) status = esp_wifi_set_mode(WIFI_MODE_STA);
    if (status == ESP_OK) status = esp_wifi_set_config(WIFI_IF_STA, &config);
    if (status == ESP_OK) status = esp_wifi_start();
    if (status == ESP_OK) status = esp_wifi_connect();
    return return_bool(result, status == ESP_OK);
}
#endif

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
    if (name_equals(name, name_len, "gpio::configure") && arg_count == 2 &&
        is_gpio(args[0]) && is_int(args[1]) &&
        (args[1].integer == 0 || args[1].integer == 1)) {
        gpio_config_t config{};
        config.pin_bit_mask = uint64_t{1} << args[0].integer;
        config.mode = args[1].integer == 1 ? GPIO_MODE_OUTPUT : GPIO_MODE_INPUT;
        return return_bool(result, gpio_config(&config) == ESP_OK);
    }
    if (name_equals(name, name_len, "gpio::digital_write") && arg_count == 2 &&
        is_gpio(args[0]) && args[1].tag == RUSTSCRIPT_VALUE_BOOL) {
        return return_bool(
            result,
            gpio_set_level(
                static_cast<gpio_num_t>(args[0].integer),
                args[1].boolean ? 1 : 0
            ) == ESP_OK
        );
    }
    if (name_equals(name, name_len, "gpio::digital_read") && arg_count == 1 && is_gpio(args[0])) {
        return return_bool(
            result,
            gpio_get_level(static_cast<gpio_num_t>(args[0].integer)) != 0
        );
    }
    if (name_equals(name, name_len, "mcu::delay_ms") && arg_count == 1 &&
        is_int(args[0]) && args[0].integer >= 0 && args[0].integer <= UINT32_MAX) {
        vTaskDelay(pdMS_TO_TICKS(static_cast<uint32_t>(args[0].integer)));
        return 0;
    }
    if (name_equals(name, name_len, "mcu::millis") && arg_count == 0) {
        return return_int(result, esp_timer_get_time() / 1000);
    }
    if (name_equals(name, name_len, "mcu::micros") && arg_count == 0) {
        return return_int(result, esp_timer_get_time());
    }
    if (name_equals(name, name_len, "mcu::free_heap") && arg_count == 0) {
        return return_int(result, esp_get_free_heap_size());
    }
    if (name_equals(name, name_len, "mcu::random") && arg_count == 0) {
        return return_int(result, esp_random());
    }
    if (name_equals(name, name_len, "serial::write_line") && arg_count == 1 &&
        is_string(args[0])) {
        if (args[0].len == 0) {
            std::puts("");
        } else {
            std::printf("%.*s\n", static_cast<int>(args[0].len), args[0].data);
        }
        return 0;
    }
#ifdef RUSTSCRIPT_FEATURE_WIFI
    if (name_equals(name, name_len, "wifi::connect") && arg_count == 2) {
        return wifi_connect(args, result);
    }
    if (name_equals(name, name_len, "wifi::disconnect") && arg_count == 0) {
        return return_bool(result, wifi_initialized && esp_wifi_disconnect() == ESP_OK);
    }
    if (name_equals(name, name_len, "wifi::is_connected") && arg_count == 0) {
        wifi_ap_record_t access_point{};
        return return_bool(
            result,
            wifi_initialized && esp_wifi_sta_get_ap_info(&access_point) == ESP_OK
        );
    }
    if (name_equals(name, name_len, "wifi::rssi") && arg_count == 0) {
        wifi_ap_record_t access_point{};
        if (!wifi_initialized || esp_wifi_sta_get_ap_info(&access_point) != ESP_OK) {
            return return_int(result, -127);
        }
        return return_int(result, access_point.rssi);
    }
    if (name_equals(name, name_len, "wifi::local_ip") && arg_count == 0) {
        esp_netif_ip_info_t info{};
        if (wifi_station == nullptr || esp_netif_get_ip_info(wifi_station, &info) != ESP_OK ||
            info.ip.addr == 0) {
            network_text[0] = '\0';
        } else {
            std::snprintf(network_text, sizeof(network_text), IPSTR, IP2STR(&info.ip));
        }
        return return_string(result, network_text);
    }
#endif
#ifdef RUSTSCRIPT_FEATURE_BLUETOOTH
    if (name_equals(name, name_len, "bluetooth::enable") && arg_count == 0) {
        esp_bt_controller_status_t status = esp_bt_controller_get_status();
        if (status == ESP_BT_CONTROLLER_STATUS_IDLE) {
#pragma GCC diagnostic push
#pragma GCC diagnostic ignored "-Wmissing-field-initializers"
            esp_bt_controller_config_t config = BT_CONTROLLER_INIT_CONFIG_DEFAULT();
#pragma GCC diagnostic pop
            if (esp_bt_controller_init(&config) != ESP_OK) {
                return return_bool(result, false);
            }
            status = ESP_BT_CONTROLLER_STATUS_INITED;
        }
        if (status == ESP_BT_CONTROLLER_STATUS_INITED &&
            esp_bt_controller_enable(ESP_BT_MODE_BLE) != ESP_OK) {
            return return_bool(result, false);
        }
        return return_bool(
            result,
            esp_bt_controller_get_status() == ESP_BT_CONTROLLER_STATUS_ENABLED
        );
    }
    if (name_equals(name, name_len, "bluetooth::disable") && arg_count == 0) {
        bool ok = true;
        if (esp_bt_controller_get_status() == ESP_BT_CONTROLLER_STATUS_ENABLED) {
            ok = esp_bt_controller_disable() == ESP_OK;
        }
        if (esp_bt_controller_get_status() == ESP_BT_CONTROLLER_STATUS_INITED) {
            ok = esp_bt_controller_deinit() == ESP_OK && ok;
        }
        return return_bool(result, ok);
    }
    if (name_equals(name, name_len, "bluetooth::is_enabled") && arg_count == 0) {
        return return_bool(
            result,
            esp_bt_controller_get_status() == ESP_BT_CONTROLLER_STATUS_ENABLED
        );
    }
#endif
    return -1;
}

}  // namespace

extern "C" void *rustscript_platform_alloc(size_t size, size_t align) {
    return heap_caps_aligned_alloc(align, size, MALLOC_CAP_8BIT);
}

extern "C" void rustscript_platform_dealloc(void *pointer, size_t, size_t) {
    std::free(pointer);
}

extern "C" void rust_eh_personality() {}

extern "C" void app_main() {
    const int32_t status = rustscript_run_vmbc(
        RUSTSCRIPT_PROGRAM_VMBC,
        RUSTSCRIPT_PROGRAM_VMBC_LEN,
        dispatch_host,
        nullptr,
        1000000
    );
    std::printf("rss:status=%ld\n", static_cast<long>(status));
}
