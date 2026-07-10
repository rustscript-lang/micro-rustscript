Import("env")

import os
import pathlib
import subprocess

PROJECT_DIR = pathlib.Path(env.subst("$PROJECT_DIR")).resolve()
REPO_ROOT = PROJECT_DIR.parents[1]
RUSTSCRIPT_REPO = pathlib.Path(
    os.environ.get("RUSTSCRIPT_REPO", str(REPO_ROOT.parent / "rustscript"))
).resolve()
TARGET_DIR = PROJECT_DIR / ".pio" / "rust-target"
GENERATED_DIR = PROJECT_DIR / "generated"
SOURCE = PROJECT_DIR / "programs" / "blinky.rss"
VMBC = GENERATED_DIR / "blinky.vmbc"
HEADER = GENERATED_DIR / "program_vmbc.h"
ARCHIVE = TARGET_DIR / "thumbv6m-none-eabi" / "release" / "librustscript_embedded.a"


def run(command, cwd):
    print("rustscript-embedded:", " ".join(str(item) for item in command))
    subprocess.run(command, cwd=cwd, check=True)


def write_header(payload):
    rows = []
    for offset in range(0, len(payload), 12):
        chunk = payload[offset : offset + 12]
        rows.append("    " + ", ".join(f"0x{byte:02x}" for byte in chunk) + ",")
    content = "\n".join(
        [
            "#ifndef RUSTSCRIPT_PROGRAM_VMBC_H",
            "#define RUSTSCRIPT_PROGRAM_VMBC_H",
            "",
            "#include <stddef.h>",
            "#include <stdint.h>",
            "",
            "static const uint8_t RUSTSCRIPT_PROGRAM_VMBC[] = {",
            *rows,
            "};",
            "static const size_t RUSTSCRIPT_PROGRAM_VMBC_LEN = sizeof(RUSTSCRIPT_PROGRAM_VMBC);",
            "",
            "#endif",
            "",
        ]
    )
    if not HEADER.exists() or HEADER.read_text() != content:
        HEADER.write_text(content)


GENERATED_DIR.mkdir(parents=True, exist_ok=True)
build_environment = os.environ.copy()
build_environment["CARGO_TARGET_DIR"] = str(TARGET_DIR)

print("rustscript-embedded: building thumbv6m-none-eabi static library")
subprocess.run(
    [
        "cargo",
        "build",
        "--release",
        "--target",
        "thumbv6m-none-eabi",
        "--no-default-features",
        "--features",
        "rp2040",
    ],
    cwd=REPO_ROOT,
    env=build_environment,
    check=True,
)

run(
    [
        "cargo",
        "run",
        "--quiet",
        "--manifest-path",
        str(RUSTSCRIPT_REPO / "Cargo.toml"),
        "-p",
        "pd-vm",
        "--bin",
        "pd-vm-run",
        "--",
        str(SOURCE),
        "--emit-vmbc",
        str(VMBC),
    ],
    RUSTSCRIPT_REPO,
)

if not ARCHIVE.is_file() or ARCHIVE.stat().st_size == 0:
    raise RuntimeError(f"missing Rust static library: {ARCHIVE}")
if not VMBC.is_file() or VMBC.stat().st_size == 0:
    raise RuntimeError(f"missing VMBC output: {VMBC}")
write_header(VMBC.read_bytes())

env.Append(CPPPATH=[str(REPO_ROOT / "include"), str(GENERATED_DIR)])
env.Append(LIBS=[env.File(str(ARCHIVE))])
