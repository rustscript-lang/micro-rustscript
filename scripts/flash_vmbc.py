#!/usr/bin/env python3

import argparse
import pathlib
import shutil
import subprocess
import sys
import tempfile

from vmbc_image import find_partition, pack_vmbc


def compile_rss(source, output, project_dir):
    installed = shutil.which("rustscript-compile-vmbc")
    if installed:
        command = [installed, str(source), str(output)]
        cwd = None
    elif (project_dir / "Cargo.toml").is_file():
        command = [
            "cargo",
            "run",
            "--quiet",
            "--bin",
            "rustscript-compile-vmbc",
            "--",
            str(source),
            str(output),
        ]
        cwd = project_dir
    else:
        raise RuntimeError(
            "RSS compilation needs rustscript-compile-vmbc on PATH or a micro-rustscript checkout"
        )
    subprocess.run(command, cwd=cwd, check=True)


def main():
    script_dir = pathlib.Path(__file__).resolve().parent
    project_dir = script_dir.parent
    default_partitions = project_dir / "partitions.csv"
    if not default_partitions.is_file():
        default_partitions = script_dir / "partitions.csv"
    parser = argparse.ArgumentParser(
        description="Compile/package and independently flash VMBC into the ESP32 script partition."
    )
    parser.add_argument("input", type=pathlib.Path, help=".vmbc or .rss input")
    parser.add_argument("--port", help="serial port; omit to only create the partition image")
    parser.add_argument("--output", type=pathlib.Path, help="write the packed partition image here")
    parser.add_argument("--partitions", type=pathlib.Path, default=default_partitions)
    parser.add_argument("--chip", default="esp32c3")
    parser.add_argument("--baud", default="921600")
    args = parser.parse_args()

    offset, partition_size = find_partition(args.partitions)
    with tempfile.TemporaryDirectory(prefix="rustscript-vmbc-") as temporary:
        temporary = pathlib.Path(temporary)
        if args.input.suffix.lower() == ".rss":
            vmbc = temporary / (args.input.stem + ".vmbc")
            compile_rss(args.input.resolve(), vmbc, project_dir)
            payload = vmbc.read_bytes()
        else:
            payload = args.input.read_bytes()
        image = pack_vmbc(payload, partition_size)
        output = args.output or temporary / "rustscript.partition.bin"
        output.parent.mkdir(parents=True, exist_ok=True)
        output.write_bytes(image)
        print(
            f"packed {len(payload)} VMBC bytes into {len(image)} bytes "
            f"for offset 0x{offset:x}"
        )
        if args.output:
            print(f"wrote {output}")
        if args.port:
            command = [
                sys.executable,
                "-m",
                "esptool",
                "--chip",
                args.chip,
                "--port",
                args.port,
                "--baud",
                args.baud,
                "write_flash",
                hex(offset),
                str(output),
            ]
            subprocess.run(command, check=True)
        elif not args.output:
            parser.error("provide --port, --output, or both")


if __name__ == "__main__":
    main()
