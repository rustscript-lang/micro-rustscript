Import("env")

import os
import pathlib
import subprocess
import sys

PROJECT_DIR = pathlib.Path(env.subst("$PROJECT_DIR")).resolve()
TARGET_DIR = PROJECT_DIR / ".pio" / "rust-target"
GENERATED_DIR = PROJECT_DIR / ".pio" / "generated"
SOURCE = PROJECT_DIR / "programs" / "esp32-blinky.rss"
VMBC = GENERATED_DIR / "esp32-blinky.vmbc"
SCRIPT_IMAGE = GENERATED_DIR / "rustscript.partition.bin"
PARTITIONS = PROJECT_DIR / "partitions.csv"
RUST_TARGET = "riscv32imc-unknown-none-elf"
ARCHIVE = TARGET_DIR / RUST_TARGET / "release" / "librustscript_embedded.a"

sys.path.insert(0, str(PROJECT_DIR / "scripts"))
from vmbc_image import find_partition, pack_vmbc


def run(command, cwd, environment=None):
    print("micro-rustscript:", " ".join(str(item) for item in command))
    clean_env = os.environ.copy() if environment is None else environment.copy()
    clean_env.pop("RUSTFLAGS", None)
    subprocess.run(command, cwd=cwd, env=clean_env, check=True)


def command_output(command, cwd, environment=None):
    clean_env = os.environ.copy() if environment is None else environment.copy()
    clean_env.pop("RUSTFLAGS", None)
    return subprocess.check_output(command, cwd=cwd, env=clean_env, text=True).strip()


GENERATED_DIR.mkdir(parents=True, exist_ok=True)
build_environment = os.environ.copy()
build_environment["CARGO_TARGET_DIR"] = str(TARGET_DIR)
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
        "esp32c3",
    ],
    PROJECT_DIR,
    build_environment,
)

if not ARCHIVE.is_file() or ARCHIVE.stat().st_size == 0:
    raise RuntimeError(f"missing Rust static library: {ARCHIVE}")

rust_sysroot = pathlib.Path(
    command_output(["rustc", "--print", "sysroot"], PROJECT_DIR, build_environment)
)
rust_host = command_output(["rustc", "--print", "host-tuple"], PROJECT_DIR, build_environment)
llvm_objcopy = rust_sysroot / "lib" / "rustlib" / rust_host / "bin" / "llvm-objcopy"
if not llvm_objcopy.is_file():
    raise RuntimeError(
        "llvm-objcopy is required; install it with `rustup component add llvm-tools-preview`"
    )
run(
    [str(llvm_objcopy), "--remove-section=.riscv.attributes", str(ARCHIVE)],
    PROJECT_DIR,
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
        str(SOURCE),
        str(VMBC),
    ],
    PROJECT_DIR,
    build_environment,
)

if not VMBC.is_file() or VMBC.stat().st_size == 0:
    raise RuntimeError(f"missing VMBC output: {VMBC}")
script_offset, script_size = find_partition(PARTITIONS)
SCRIPT_IMAGE.write_bytes(pack_vmbc(VMBC.read_bytes(), script_size))
print(
    "micro-rustscript: packed default VMBC partition",
    SCRIPT_IMAGE,
    f"(offset 0x{script_offset:x})",
)

env.Append(CPPPATH=[str(PROJECT_DIR / "include")])
env.Append(LIBS=[env.File(str(ARCHIVE))])
env.Append(FLASH_EXTRA_IMAGES=[(hex(script_offset), env.File(str(SCRIPT_IMAGE)))])
