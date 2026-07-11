#!/usr/bin/env bash
set -euo pipefail

IDF_COMMIT=e88643bc619dff3c21b51705113bb488c9bc0990
IDF_PATH=${ESP_IDF_PATH:-/mnt/TEMP/esp-idf}
IDF_TOOLS_PATH=${IDF_TOOLS_PATH:-/mnt/TEMP/esp-idf-tools}

export IDF_TOOLS_PATH
export GIT_CONFIG_GLOBAL=/dev/null
unset IDF_PYTHON_ENV_PATH
mkdir -p /mnt/TEMP "$IDF_TOOLS_PATH"

if [[ ! -d "$IDF_PATH/.git" ]]; then
  git clone --depth 1 --branch master \
    https://github.com/espressif/esp-idf.git "$IDF_PATH"
fi

git -C "$IDF_PATH" fetch --depth 1 origin "$IDF_COMMIT"
git -C "$IDF_PATH" checkout --detach "$IDF_COMMIT"
for attempt in 1 2 3; do
  if git -C "$IDF_PATH" submodule update --init --recursive --depth 1; then
    break
  fi
  if [[ "$attempt" == 3 ]]; then
    exit 1
  fi
  sleep $((attempt * 5))
done

"$IDF_PATH/install.sh" esp32s31
printf 'ESP-IDF %s installed under %s\n' "$IDF_COMMIT" /mnt/TEMP
