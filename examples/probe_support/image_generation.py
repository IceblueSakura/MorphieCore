"""Fixed static image scenarios and bounded PNG raster checks, without body capture."""
import base64
import binascii
import json
import struct
import time
import zlib
from .checks import ProbeFailure, require
from .ledger import MODELS
from .runtime import session

CASES = {
    "minimal": ("blue", {}),
    "explicit": ("red", {"n": 1, "stream": False}),
    "nullable": ("blue", {"n": None, "stream": None, "output_format": None}),
    "pair": ("blue", {"n": 2, "stream": False}),
}


def inspect_png(png, color):
    """Decode bounded non-interlaced 8-bit RGB/RGBA PNGs, never infer metadata."""
    require(color in ("blue", "red"), "image_color", "setup")
    require(len(png) <= 2 << 20 and png.startswith(b"\x89PNG\r\n\x1a\n"), "image_format")
    pos, chunks, header, ended, body_closed = 8, 0, None, False, False
    compressed = bytearray()
    idat_seen = False
    while pos < len(png):
        require(pos + 12 <= len(png) and not ended, "image_decode")
        length = struct.unpack_from(">I", png, pos)[0]
        kind = png[pos + 4:pos + 8]
        end = pos + 8 + length
        require(end + 4 <= len(png), "image_decode")
        require(all(65 <= c <= 90 or 97 <= c <= 122 for c in kind) and not kind[2] & 32 and kind not in (b"acTL", b"fcTL", b"fdAT"), "image_format")
        data = png[pos + 8:end]
        require((zlib.crc32(kind + data) & 0xffffffff) == struct.unpack_from(">I", png, end)[0], "image_decode")
        chunks += 1
        require(chunks <= 4096, "image_decode")
        if kind == b"IHDR":
            require(chunks == 1 and length == 13, "image_decode")
            width, height, depth, mode, compression, filtering, interlace = struct.unpack(">IIBBBBB", data)
            require(0 < width <= 8192 and 0 < height <= 8192 and width * height <= 16_777_216, "image_decode")
            require(depth == 8 and mode in (2, 6) and compression == filtering == interlace == 0, "image_format")
            header = (width, height, 3 if mode == 2 else 4)
        else:
            require(header is not None, "image_decode")
            if kind == b"IDAT":
                require(not body_closed, "image_decode")
                idat_seen = True
                compressed.extend(data)
            else:
                if idat_seen:
                    body_closed = True
                if kind == b"IEND":
                    require(length == 0 and bool(compressed), "image_decode")
                    ended = True
                elif kind == b"PLTE":
                    require(not idat_seen and 0 < length <= 768 and length % 3 == 0, "image_decode")
                else:
                    require(bool(kind[0] & 32), "image_format")
        pos = end + 4
    require(ended and header is not None, "image_decode")
    width, height, channels = header
    stride = width * channels
    expected = (stride + 1) * height
    inflater = zlib.decompressobj()
    try:
        raster = inflater.decompress(compressed, expected + 1)
    except zlib.error:
        raise ProbeFailure("image_decode") from None
    require(len(raster) == expected and inflater.eof and not inflater.unused_data and not inflater.unconsumed_tail, "image_decode")
    previous, colored = bytearray(stride), 0
    for y in range(height):
        start = y * (stride + 1)
        filter_kind = raster[start]
        require(filter_kind <= 4, "image_decode")
        row = bytearray(raster[start + 1:start + 1 + stride])
        if filter_kind:
            for x in range(stride):
                left = row[x - channels] if x >= channels else 0
                up = previous[x]
                upper_left = previous[x - channels] if x >= channels else 0
                if filter_kind == 1:
                    predictor = left
                elif filter_kind == 2:
                    predictor = up
                elif filter_kind == 3:
                    predictor = (left + up) // 2
                else:
                    p = left + up - upper_left
                    distances = (abs(p - left), abs(p - up), abs(p - upper_left))
                    predictor = (left, up, upper_left)[distances.index(min(distances))]
                row[x] = (row[x] + predictor) & 255
        for x in range(0, stride, channels):
            red, green, blue = row[x:x + 3]
            if channels == 4 and row[x + 3] < 128:
                continue
            target, others = (blue, (red, green)) if color == "blue" else (red, (green, blue))
            colored += target >= 150 and target > 1.5 * max(others)
        previous = row
    return {"image_decoded": True, "image_count": 1, "image_bytes": len(png),
            "image_width": width, "image_height": height,
            "content_ok": colored >= max(1, width * height // 100)}


def plan_groups(run, models, *, cases=("image_generate",), protocol=None, delivery=None, effort=None):
    require(run.is_images and protocol in (None, "images") and delivery in (None, "json") and effort is None, "image_selection", "setup")
    # The original matrix never silently grows paid work.
    selected = ["minimal", "explicit", "nullable"] if cases == ("image_generate",) else [c.removeprefix("image_generate_") for c in cases]
    require(selected and len(set(selected)) == len(selected) and all(c in CASES for c in selected), "image_cases", "setup")
    require(all((CASES[c][1].get("n") or 1) == run.plan["images_per_request"] for c in selected), "image_count", "budget")
    require(models and len(set(models)) == len(models), "image_models", "setup")
    groups = []
    for model in models:
        require(model in run.plan["models"] and MODELS[model][4] == ("images",), "image_selection", "budget")
        for case in selected:
            groups.append((MODELS[model][0], "images", False, case, f"{model}:images:{case}", 1, None))
    run.register([(scenario.split(":", 1)[0], scenario, cap) for _, _, _, _, scenario, _, cap in groups])
    return groups


def inspect_result(result, color, count):
    """Validate all artifacts before reporting success; no first-item proxy."""
    require(type(count) is int and 1 <= count <= 10, "image_count", "setup")
    require(type(result.created) is int and result.created >= 0
            and result.data is not None and len(result.data) == count, "image_response", "wire")
    require(result.output_format in (None, "png"), "image_format")
    total, dimensions = 0, set()
    for image in result.data:
        require(image.url is None and isinstance(image.b64_json, str)
                and 0 < len(image.b64_json) <= 2_796_204, "image_response", "wire")
        require(len(image.b64_json) <= ((2 * 1024 * 1024 - total + 2) // 3) * 4, "image_decode")
        try:
            data = base64.b64decode(image.b64_json, validate=True)
        except (binascii.Error, ValueError):
            raise ProbeFailure("image_decode") from None
        total += len(data)
        require(total <= 2 << 20, "image_decode")
        facts = inspect_png(data, color)
        dimensions.add((facts["image_width"], facts["image_height"]))
        require(result.size is None or result.size == f"{facts['image_width']}x{facts['image_height']}", "image_decode")
        require(facts["content_ok"], "image_pixels")
    metrics = {"image_decoded": True, "image_count": count, "image_bytes": total, "content_ok": True}
    if len(dimensions) == 1:
        metrics["image_width"], metrics["image_height"] = dimensions.pop()
    return metrics


def matrix(run, models, **options):
    groups = plan_groups(run, models, **options)
    success = True
    with session(run, models) as (client, transport):
        for _, _, _, case, scenario, _, _ in groups:
            model = scenario.split(":", 1)[0]
            color, controls = CASES[case]
            prompt = f"A single solid {color} square centered on a plain white background. No text, no other objects."
            transport.prepare(model, "images", scenario, prompt)
            started = time.monotonic()
            metrics = {"sdk_consumed": False, "image_decoded": False, "content_ok": False}
            try:
                result = client.images.generate(model=model, prompt=prompt, **controls)
                metrics["sdk_consumed"] = True
                require(transport.wire.closed, "image_eof", "wire")
                facts = inspect_result(result, color, run.plan["images_per_request"])
                metrics.update(facts, terminal="image.complete")
                metrics["elapsed_ms"] = round((time.monotonic() - started) * 1000)
                identity = transport.complete("passed", metrics)
                print(json.dumps({"attempt": identity, "state": "passed", **facts}), flush=True)
            except Exception as error:
                kind = error.kind if isinstance(error, ProbeFailure) else ("http" if transport.last_status and transport.last_status >= 400 else "consumer")
                metrics.update(failure=kind, elapsed_ms=round((time.monotonic() - started) * 1000))
                if kind == "oracle":
                    code = getattr(error, "code", "other")
                    metrics["oracle_failure"] = code if code in ("image_format", "image_decode", "image_pixels") else "other"
                if transport.attempt is not None:
                    identity = transport.complete("oracle_failed" if kind == "oracle" else "failed", metrics)
                    print(json.dumps({"attempt": identity, "state": "failed", "failure": kind}), flush=True)
                success = False
                if kind != "oracle" or not run.plan["continue_oracle"]:
                    break
    return success
