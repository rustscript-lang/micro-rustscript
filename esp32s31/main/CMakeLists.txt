idf_component_register(
    SRCS "main.cpp"
    INCLUDE_DIRS "." "../../include" "$ENV{RUSTSCRIPT_GENERATED_DIR}"
    REQUIRES bt esp_driver_gpio esp_event esp_netif esp_timer esp_wifi nvs_flash spi_flash
)

if(RUSTSCRIPT_FEATURE_WIFI)
    target_compile_definitions(${COMPONENT_LIB} PRIVATE RUSTSCRIPT_FEATURE_WIFI=1)
endif()
if(RUSTSCRIPT_FEATURE_BLUETOOTH)
    target_compile_definitions(${COMPONENT_LIB} PRIVATE RUSTSCRIPT_FEATURE_BLUETOOTH=1)
endif()

add_prebuilt_library(rustscript_embedded "$ENV{RUSTSCRIPT_ARCHIVE}")
target_link_libraries(${COMPONENT_LIB} PRIVATE rustscript_embedded)
