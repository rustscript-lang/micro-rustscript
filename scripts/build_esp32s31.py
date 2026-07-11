Import("env")

import os
import pathlib
import shlex
import shutil
import subprocess

PROJECT_DIR = pathlib.Path(env.subst("$PROJECT_DIR")).resolve()
IDF_PATH = pathlib.Path(os.environ.get("ESP_IDF_PATH", "/mnt/TEMP/esp-idf")).resolve()
IDF_TOOLS_PATH = pathlib.Path(
    os.environ.get("IDF_TOOLS_PATH", "/mnt/TEMP/esp-idf-tools")
).resolve()
WORK_DIR = pathlib.Path(
    os.environ.get("ESP32S31_WORK_DIR", "/mnt/TEMP/micro-rustscript-esp32s31")
).resolve()
BUILD_DIR = WORK_DIR / "build"
SDKCONFIG = WORK_DIR / "sdkconfig"
RUST_TARGET_DIR = WORK_DIR / "rust-target"
GENERATED_DIR = WORK_DIR / "generated"
RUST_TARGET = "riscv32imafc-unknown-none-elf"
RUST_FEATURES = [
    feature.strip()
    for feature in env.GetProjectOption(
        "custom_rust_features", "esp32s31,wifi,bluetooth"
    ).split(",")
    if feature.strip()
]
ARCHIVE = RUST_TARGET_DIR / RUST_TARGET / "release" / "librustscript_embedded.a"
VMBC = GENERATED_DIR / "program.vmbc"
HEADER = GENERATED_DIR / "program_vmbc.h"
DIST = PROJECT_DIR / "dist" / "micro-rustscript-esp32-s31.factory.bin"


def run(command, environment=None):
    print("micro-rustscript-s31:", " ".join(str(item) for item in command))
    clean_env = os.environ.copy() if environment is None else environment.copy()
    clean_env.pop("RUSTFLAGS", None)
    subprocess.run(command, cwd=PROJECT_DIR, env=clean_env, check=True)


def command_output(command, environment=None):
    clean_env = os.environ.copy() if environment is None else environment.copy()
    clean_env.pop("RUSTFLAGS", None)
    return subprocess.check_output(
        command, cwd=PROJECT_DIR, env=clean_env, text=True
    ).strip()


def write_header(payload):
    rows = []
    for offset in range(0, len(payload), 12):
        chunk = payload[offset : offset + 12]
        rows.append("    " + ", ".join(f"0x{byte:02x}" for byte in chunk) + ",")
    HEADER.write_text(
        "\n".join(
            [
                "#pragma once",
                "#include <stddef.h>",
                "#include <stdint.h>",
                "static const uint8_t RUSTSCRIPT_PROGRAM_VMBC[] = {",
                *rows,
                "};",
                "static const size_t RUSTSCRIPT_PROGRAM_VMBC_LEN = sizeof(RUSTSCRIPT_PROGRAM_VMBC);",
                "",
            ]
        )
    )


if not (IDF_PATH / "export.sh").is_file():
    raise RuntimeError(
        f"ESP-IDF is missing at {IDF_PATH}; install the pinned ESP-IDF into /mnt/TEMP first"
    )

GENERATED_DIR.mkdir(parents=True, exist_ok=True)
RUST_TARGET_DIR.mkdir(parents=True, exist_ok=True)
build_environment = os.environ.copy()
build_environment["CARGO_TARGET_DIR"] = str(RUST_TARGET_DIR)
build_environment.pop("RUSTFLAGS", None)

run(
    [
        "cargo",
        "build",
        "--release",
        "--target",
        RUST_TARGET,
        "--no-default-features",
        "--features",
        ",".join(RUST_FEATURES),
    ],
    build_environment,
)
run(
    [
        "cargo",
        "run",
        "--quiet",
        "--bin",
        "rustscript-compile-vmbc",
        "--",
        str(PROJECT_DIR / "programs" / "esp32-blinky.rss"),
        str(VMBC),
    ],
    build_environment,
)
write_header(VMBC.read_bytes())

rust_sysroot = pathlib.Path(
    command_output(["rustc", "--print", "sysroot"], build_environment)
)
rust_host = command_output(["rustc", "--print", "host-tuple"], build_environment)
llvm_objcopy = rust_sysroot / "lib" / "rustlib" / rust_host / "bin" / "llvm-objcopy"
run([str(llvm_objcopy), "--remove-section=.riscv.attributes", str(ARCHIVE)])

idf_environment = os.environ.copy()
idf_environment.pop("IDF_PYTHON_ENV_PATH", None)
idf_environment.pop("IDF_PATH", None)
idf_environment.update(
    {
        "IDF_PATH": str(IDF_PATH),
        "IDF_TOOLS_PATH": str(IDF_TOOLS_PATH),
        "RUSTSCRIPT_ARCHIVE": str(ARCHIVE),
        "RUSTSCRIPT_GENERATED_DIR": str(GENERATED_DIR),
    }
)
command = (
    "unset IDF_PYTHON_ENV_PATH IDF_PATH; "
    f"export IDF_TOOLS_PATH={shlex.quote(str(IDF_TOOLS_PATH))}; "
    f"source {shlex.quote(str(IDF_PATH / 'export.sh'))} >/dev/null && "
    f"idf.py --preview -C {shlex.quote(str(PROJECT_DIR / 'esp32s31'))} "
    f"-B {shlex.quote(str(BUILD_DIR))} "
    f"-D SDKCONFIG={shlex.quote(str(SDKCONFIG))} "
    f"-D RUSTSCRIPT_FEATURE_WIFI={'ON' if 'wifi' in RUST_FEATURES else 'OFF'} "
    f"-D RUSTSCRIPT_FEATURE_BLUETOOTH={'ON' if 'bluetooth' in RUST_FEATURES else 'OFF'} "
    f"{'build merge-bin' if SDKCONFIG.is_file() else 'set-target esp32s31 build merge-bin'}"
)
run(["bash", "-lc", command], idf_environment)

merged = BUILD_DIR / "merged-binary.bin"
if not merged.is_file():
    raise RuntimeError(f"ESP-IDF did not create a merged ESP32-S31 image at {merged}")
DIST.parent.mkdir(parents=True, exist_ok=True)
shutil.copy2(merged, DIST)
print(f"micro-rustscript-s31: created {DIST} ({DIST.stat().st_size} bytes)")
