#!/usr/bin/env python3

import argparse
import binascii
import pathlib
import time


def wait_for_line(serial_port, expected, timeout=10):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        line = serial_port.readline().decode("utf-8", errors="replace").strip()
        if line:
            print(line)
        if line == expected:
            return
    raise TimeoutError(f"device did not send {expected!r}")


def main():
    parser = argparse.ArgumentParser(description="Send VMBC to the firmware VMBC REPL.")
    parser.add_argument("input", type=pathlib.Path)
    parser.add_argument("--port", required=True)
    parser.add_argument("--baud", type=int, default=115200)
    parser.add_argument("--install", action="store_true", help="persist to the script partition")
    args = parser.parse_args()

    try:
        import serial
    except ImportError as error:
        raise SystemExit("install pyserial first: python -m pip install pyserial") from error

    payload = args.input.read_bytes()
    crc = binascii.crc32(payload) & 0xFFFFFFFF
    command = "install" if args.install else "load"
    with serial.Serial(args.port, args.baud, timeout=0.25) as device:
        device.reset_input_buffer()
        device.write(f"{command} {len(payload)} {crc:08x}\n".encode())
        device.flush()
        wait_for_line(device, "rss:ready")
        device.write(payload)
        device.flush()
        deadline = time.monotonic() + 30
        while time.monotonic() < deadline:
            line = device.readline().decode("utf-8", errors="replace").strip()
            if line:
                print(line)
            if line.startswith("rss:status=") or line.startswith("rss:error="):
                break


if __name__ == "__main__":
    main()
