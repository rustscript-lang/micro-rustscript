Import("env")

import pathlib

PROJECT_DIR = pathlib.Path(env.subst("$PROJECT_DIR")).resolve()
DIST_DIR = PROJECT_DIR / "dist"
OUTPUT = DIST_DIR / "micro-rustscript-esp32-c3.factory.bin"
SCRIPT_ENV = env


def resolve_image(build_env, image):
    return pathlib.Path(build_env.subst(str(image))).resolve()


def merge_firmware(source, target, build_env=None, **kwargs):
    if build_env is None:
        build_env = kwargs.get("env", SCRIPT_ENV)

    images = []
    for offset, image in build_env["FLASH_EXTRA_IMAGES"]:
        images.append((int(build_env.subst(str(offset)), 0), resolve_image(build_env, image)))

    app = pathlib.Path(build_env.subst("$BUILD_DIR")) / f"{build_env.subst('$PROGNAME')}.bin"
    images.append((int(build_env.subst("$ESP32_APP_OFFSET"), 0), app.resolve()))
    images.sort(key=lambda item: item[0])

    for _, image in images:
        if not image.is_file() or image.stat().st_size == 0:
            raise RuntimeError(f"missing firmware image: {image}")

    DIST_DIR.mkdir(parents=True, exist_ok=True)
    with OUTPUT.open("wb") as merged:
        cursor = 0
        for offset, image in images:
            if offset < cursor:
                raise RuntimeError(
                    f"firmware image overlap at 0x{offset:x}: {image} starts before 0x{cursor:x}"
                )
            remaining = offset - cursor
            while remaining:
                chunk_size = min(remaining, 64 * 1024)
                merged.write(b"\xff" * chunk_size)
                remaining -= chunk_size
            payload = image.read_bytes()
            merged.write(payload)
            cursor = offset + len(payload)

    print(
        "micro-rustscript: created",
        OUTPUT,
        f"({OUTPUT.stat().st_size} bytes, flash offset 0x0)",
    )


env.AddPostAction("$BUILD_DIR/${PROGNAME}.bin", merge_firmware)
