#!/usr/bin/env python3
"""
RustScript serial REPL client.

Compiles each line on the host with the full RustScript compiler, sends VMBC +
serialised locals to the device, receives updated locals + result, and prints
"=> value".  Locals are preserved across lines so bindings carry over.
"""
from __future__ import annotations

import argparse
import pathlib
import readline
import struct
import subprocess
import sys
import tempfile
import time

COMPILE_BINARY = pathlib.Path(__file__).resolve().parent.parent / "target" / "release" / "rustscript-repl-compile"

# When the pre-built binary is missing, fall back to cargo run.
_COMPILE_FALLBACK: list[str] | None = None

def _compile_vmbc(source: str, timeout: int = 120) -> bytes:
    global _COMPILE_FALLBACK
    if COMPILE_BINARY.is_file():
        cmd = [str(COMPILE_BINARY)]
    elif _COMPILE_FALLBACK is not None:
        cmd = _COMPILE_FALLBACK
    else:
        result = subprocess.run(
            ["cargo", "build", "--release", "--quiet", "--bin", "rustscript-repl-compile"],
            cwd=COMPILE_BINARY.parent.parent,
            capture_output=True, timeout=300,
        )
        if result.returncode != 0:
            raise SystemExit("compiler binary build failed")
        _COMPILE_FALLBACK = cmd = ["cargo", "run", "--release", "--quiet", "--bin", "rustscript-repl-compile"]
    result = subprocess.run(cmd, input=source.encode(), capture_output=True, timeout=timeout)
    if result.returncode != 0:
        msg = result.stderr.decode("utf-8", errors="replace").strip()
        raise RuntimeError(f"compilation failed: {msg}")
    return result.stdout


# ---- REPL wire protocol (mirrors src/repl_wire.rs) ----

STATE_MAGIC = b"RSR1"
RESPONSE_MAGIC = b"RSO1"
MAX_NESTING = 32


class ReplWireError(Exception):
    pass


def _read_u32(source: bytes, offset: int) -> tuple[int, int]:
    return struct.unpack_from("<I", source, offset)[0], offset + 4


def _read_u8(source: bytes, offset: int) -> tuple[int, int]:
    return source[offset], offset + 1


def _read_exact(source: bytes, offset: int, length: int) -> tuple[bytes, int]:
    end = offset + length
    if end > len(source):
        raise ReplWireError(f"truncated: need {length} bytes at offset {offset}, have {len(source)}")
    return source[offset:end], end


def _decode_value(source: bytes, offset: int, depth: int = 0) -> tuple:
    if depth > MAX_NESTING:
        raise ReplWireError("nesting too deep")
    tag, offset = _read_u8(source, offset)
    if tag == 0:
        return None, offset
    if tag == 1:
        (val,) = struct.unpack_from("<q", source, offset)
        return val, offset + 8
    if tag == 2:
        (raw,) = struct.unpack_from("<Q", source, offset)
        return struct.unpack("<d", struct.pack("<Q", raw))[0], offset + 8
    if tag == 3:
        byte, offset = _read_u8(source, offset)
        return {0: False, 1: True}.get(byte) or (_ := ReplWireError(f"invalid bool {byte}")), offset
    if tag == 4:
        length, offset = _read_u32(source, offset)
        data, offset = _read_exact(source, offset, length)
        return data.decode("utf-8"), offset
    if tag == 5:
        length, offset = _read_u32(source, offset)
        data, offset = _read_exact(source, offset, length)
        return bytearray(data), offset
    if tag == 6:
        count, offset = _read_u32(source, offset)
        vals = []
        for _ in range(count):
            v, offset = _decode_value(source, offset, depth + 1)
            vals.append(v)
        return vals, offset
    if tag == 7:
        count, offset = _read_u32(source, offset)
        entries = []
        for _ in range(count):
            k, offset = _decode_value(source, offset, depth + 1)
            v, offset = _decode_value(source, offset, depth + 1)
            entries.append((k, v))
        return dict(entries), offset
    raise ReplWireError(f"unknown tag {tag}")


def decode_response(source: bytes) -> tuple[list, object | None]:
    if len(source) < 8 or source[:4] != RESPONSE_MAGIC:
        raise ReplWireError("invalid response magic")
    count, offset = _read_u32(source, 4)
    locals_ = []
    for _ in range(count):
        v, offset = _decode_value(source, offset)
        locals_.append(v)
    has_result, offset = _read_u8(source, offset)
    result = None
    if has_result:
        result, offset = _decode_value(source, offset)
    if offset != len(source):
        raise ReplWireError("trailing response data")
    return locals_, result


def _embed_u32(output: bytearray, value: int) -> None:
    output.extend(struct.pack("<I", value))


def _embed_value(output: bytearray, value: object) -> None:
    if value is None:
        output.append(0)
    elif isinstance(value, bool):
        output.append(3)
        output.append(1 if value else 0)
    elif isinstance(value, int):
        output.append(1)
        output.extend(struct.pack("<q", value))
    elif isinstance(value, float):
        output.append(2)
        output.extend(struct.pack("<Q", struct.unpack("<Q", struct.pack("<d", value))[0]))
    elif isinstance(value, str):
        data = value.encode("utf-8")
        output.append(4)
        output.extend(struct.pack("<I", len(data)))
        output.extend(data)
    elif isinstance(value, (bytes, bytearray)):
        output.append(5)
        output.extend(struct.pack("<I", len(value)))
        output.extend(value)
    elif isinstance(value, list):
        output.append(6)
        _embed_u32(output, len(value))
        for item in value:
            _embed_value(output, item)
    elif isinstance(value, dict):
        output.append(7)
        _embed_u32(output, len(value))
        for k, v in value.items():
            _embed_value(output, k)
            _embed_value(output, v)
    else:
        raise ReplWireError(f"unsupported type: {type(value)}")


def encode_state(locals_: list) -> bytes:
    output = bytearray()
    output.extend(STATE_MAGIC)
    _embed_u32(output, len(locals_))
    for v in locals_:
        _embed_value(output, v)
    return bytes(output)


def format_value(value: object) -> str:
    if value is None:
        return "null"
    if isinstance(value, bool):
        return "true" if value else "false"
    if isinstance(value, int):
        return str(value)
    if isinstance(value, float):
        return str(value)
    if isinstance(value, str):
        return value
    if isinstance(value, (bytes, bytearray)):
        return "b" + repr(bytes(value))
    if isinstance(value, list):
        return "[" + ", ".join(format_value(v) for v in value) + "]"
    if isinstance(value, dict):
        return "{" + ", ".join(f"{format_value(k)}: {format_value(v)}" for k, v in value.items()) + "}"
    return repr(value)


# ---- Binding metadata for the compiler helper ----

SchemaTag = {
    "null": 0, "int": 1, "float": 2, "bool": 3, "string": 4, "bytes": 5,
    "array": 6, "map": 7,
}
TagSchema = {v: k for k, v in SchemaTag.items()}


def value_schema_tag(value: object) -> int:
    if value is None:
        return 0
    if isinstance(value, bool):
        return 3
    if isinstance(value, int):
        return 1
    if isinstance(value, float):
        return 2
    if isinstance(value, str):
        return 4
    if isinstance(value, (bytes, bytearray)):
        return 5
    if isinstance(value, list):
        return 6
    if isinstance(value, dict):
        return 7
    return 8  # Unknown


def type_annotation(value: object) -> str:
    tag = value_schema_tag(value)
    return {0: "null", 1: "int", 2: "float", 3: "bool", 4: "string",
            5: "bytes", 6: "[]", 7: "{}", 8: "any"}.get(tag, "int")


def main() -> None:
    parser = argparse.ArgumentParser(description="RustScript serial REPL")
    parser.add_argument("--port", required=True)
    parser.add_argument("--baud", type=int, default=115200)
    args = parser.parse_args()

    try:
        import serial
    except ImportError:
        raise SystemExit("install pyserial: python -m pip install pyserial")

    if not COMPILE_BINARY.is_file():
        raise SystemExit(f"compiler binary not found at {COMPILE_BINARY}; run 'cargo build --release --bin rustscript-repl-compile' first")

    dev = serial.Serial(args.port, args.baud, timeout=2)
    dev.reset_input_buffer()
    time.sleep(0.1)

    dev.timeout = 5
    if b"pd-vm" not in dev.read_until(b"> "):
        dev.timeout = 2
        if b"rss:" not in (dev.read(200) or b""):
            raise SystemExit("firmware did not send REPL prompt")

    known: dict[str, object] = {}
    histfile = pathlib.Path(tempfile.gettempdir()) / ".rss-repl-history"
    try:
        readline.read_history_file(str(histfile))
    except (FileNotFoundError, OSError):
        pass
    readline.set_history_length(500)

    print("RustScript REPL (ctrl+D / .quit to exit)")
    try:
        while True:
            try:
                line = input("rss> ")
            except EOFError:
                print()
                break
            trimmed = line.strip()
            if not trimmed:
                continue
            if trimmed in (".quit", ".exit"):
                break
            try:
                readline.write_history_file(str(histfile))
            except OSError:
                pass

            # Prefix known locals as let bindings so the compiler sees them.
            prefix_bindings = ""
            for name, val in known.items():
                ann = type_annotation(val)
                prefix_bindings += f"let {name}: {ann} = {_literal(val)};\n"

            source = prefix_bindings + trimmed
            try:
                vmbc = _compile_vmbc(source)
            except RuntimeError as err:
                print(f"error: {err}")
                continue

            vmbc_len = struct.unpack("<I", vmbc[:4])[0]
            vmbc_payload = vmbc[4:4 + vmbc_len]

            # Build serialized state from known values
            state = encode_state(list(known.values()))
            header = b"RSSR" + struct.pack("<II", len(vmbc_payload), len(state))
            dev.write(header + vmbc_payload + state)
            dev.flush()

            dev.timeout = 10
            output = dev.read_until(b"rss:pd-vm> ")
            if not output:
                print("error: no response from device")
                break

            decoded = output.decode("utf-8", errors="replace")
            if "rss:repl-status=" in decoded:
                status = int(decoded.split("rss:repl-status=")[1].split("\n")[0])
                print(f"device error: status={status}")
                continue

            # Look for binary response marker rss:Dxxxx
            if "rss:D" in decoded:
                # hex-encoded payload length
                idx = decoded.index("rss:D")
                hex_part = decoded[idx + 5:idx + 9]
                try:
                    payload_len = int(hex_part, 16)
                except ValueError:
                    continue
                # Read the binary payload
                payload = dev.read(payload_len)
                if len(payload) != payload_len:
                    print(f"error: expected {payload_len} response bytes, got {len(payload)}")
                    break
                try:
                    new_locals, result_val = decode_response(payload)
                except ReplWireError as err:
                    print(f"protocol error: {err}")
                    break
                # Update known locals from values
                keys = list(known.keys())
                new_known: dict[str, object] = {}
                for idx_key, key in enumerate(keys):
                    if idx_key < len(new_locals):
                        new_known[key] = new_locals[idx_key]
                known = new_known
                if result_val is not None:
                    print(f"=> {format_value(result_val)}")
            elif "rss:error=" in decoded:
                print(f"error: {decoded.split('rss:error=')[1].split(chr(10))[0]}")
    finally:
        dev.close()


def _literal(value: object) -> str:
    if value is None:
        return "null"
    if isinstance(value, bool):
        return "true" if value else "false"
    if isinstance(value, int):
        return str(value)
    if isinstance(value, float):
        return str(value)
    if isinstance(value, str):
        escaped = value.replace("\\", "\\\\").replace('"', '\\"').replace("\n", "\\n")
        return f'"{escaped}"'
    if isinstance(value, (bytes, bytearray)):
        return 'b"' + value.hex() + '"'
    if isinstance(value, list):
        return "[" + ", ".join(_literal(v) for v in value) + "]"
    if isinstance(value, dict):
        return "{" + ", ".join(f"{_literal(k)}: {_literal(v)}" for k, v in value.items()) + "}"
    return repr(value)


if __name__ == "__main__":
    main()
