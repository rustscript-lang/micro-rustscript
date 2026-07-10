import binascii
import csv
import pathlib
import struct

MAGIC = b"RSSVMBC\0"
VERSION = 1
HEADER = struct.Struct("<8sIII")
PARTITION_LABEL = "rustscript"


def parse_number(value):
    return int(value.strip(), 0)


def find_partition(partitions_csv, label=PARTITION_LABEL):
    path = pathlib.Path(partitions_csv)
    with path.open(newline="") as handle:
        rows = csv.reader(line for line in handle if not line.lstrip().startswith("#"))
        for row in rows:
            if row and row[0].strip() == label:
                if len(row) < 5:
                    raise ValueError(f"invalid partition row for {label}")
                return parse_number(row[3]), parse_number(row[4])
    raise ValueError(f"partition {label!r} not found in {path}")


def pack_vmbc(payload, partition_size=None):
    if not payload:
        raise ValueError("VMBC payload is empty")
    image = HEADER.pack(MAGIC, VERSION, len(payload), binascii.crc32(payload) & 0xFFFFFFFF) + payload
    if partition_size is not None and len(image) > partition_size:
        raise ValueError(
            f"script image is {len(image)} bytes; partition capacity is {partition_size} bytes"
        )
    return image
