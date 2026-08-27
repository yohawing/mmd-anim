from __future__ import annotations

"""Small, dependency-free PMX staging helper used by the Python runner.

MMD 9.x expects text fields in PMX files to use UTF-16LE.  The Rust parser is
encoding agnostic, but the PMM scene is opened by the Windows MMD process, so
staging has to rewrite every length-prefixed PMX text field rather than only
changing the header byte.
"""

import struct
from pathlib import Path


class PmxError(ValueError):
    pass


class _Reader:
    def __init__(self, data: bytes):
        self.data = data
        self.offset = 0

    @property
    def remaining(self) -> int:
        return len(self.data) - self.offset

    def take(self, size: int) -> bytes:
        if size < 0 or self.offset + size > len(self.data):
            raise PmxError(f"unexpected EOF at byte {self.offset}")
        chunk = self.data[self.offset : self.offset + size]
        self.offset += size
        return chunk

    def u8(self) -> int:
        return self.take(1)[0]

    def u16(self) -> int:
        return struct.unpack("<H", self.take(2))[0]

    def i32(self) -> int:
        return struct.unpack("<i", self.take(4))[0]


def stage_mmd_compatible_pmx(input_path: Path, output_path: Path) -> dict[str, object]:
    """Convert a UTF-8 PMX to UTF-16LE, preserving already-compatible files."""

    if input_path.suffix.lower() != ".pmx":
        return {"input": str(input_path), "output": str(input_path), "converted": False, "encoding": "not-pmx"}
    data = input_path.read_bytes()
    _validate_header(data)
    encoding = data[9]
    if encoding == 0:
        return {"input": str(input_path), "output": str(input_path), "converted": False, "encoding": "utf-16le"}
    if encoding != 1:
        raise PmxError(f"unsupported PMX text encoding byte: {encoding}")
    converted = convert_utf8_to_utf16(data)
    output_path.parent.mkdir(parents=True, exist_ok=True)
    output_path.write_bytes(converted)
    return {"input": str(input_path), "output": str(output_path), "converted": True, "encoding": "utf-8"}


def convert_utf8_to_utf16(data: bytes) -> bytes:
    reader = _Reader(data)
    chunks: list[bytes] = [reader.take(4), reader.take(4)]
    header_size = reader.u8()
    if header_size < 8:
        raise PmxError(f"unsupported PMX header size: {header_size}")
    chunks.append(bytes((header_size,)))
    encoding = reader.u8()
    if encoding != 1:
        raise PmxError(f"expected UTF-8 PMX encoding byte, got {encoding}")
    chunks.append(b"\x00")
    settings_bytes = reader.take(header_size - 1)
    chunks.append(settings_bytes)
    settings = {
        "additionalUvCount": settings_bytes[0] if len(settings_bytes) > 0 else 0,
        "vertexIndexSize": settings_bytes[1] if len(settings_bytes) > 1 else 4,
        "textureIndexSize": settings_bytes[2] if len(settings_bytes) > 2 else 4,
        "materialIndexSize": settings_bytes[3] if len(settings_bytes) > 3 else 4,
        "boneIndexSize": settings_bytes[4] if len(settings_bytes) > 4 else 4,
        "morphIndexSize": settings_bytes[5] if len(settings_bytes) > 5 else 4,
        "rigidBodyIndexSize": settings_bytes[6] if len(settings_bytes) > 6 else 4,
    }

    for _ in range(4):
        _convert_text(reader, chunks)
    _copy_vertices(reader, chunks, settings)
    _copy_counted_fixed(reader, chunks, settings["vertexIndexSize"])
    _copy_text_section(reader, chunks)
    _copy_materials(reader, chunks, settings)
    _copy_bones(reader, chunks, settings)
    _copy_morphs(reader, chunks, settings)
    _copy_display_frames(reader, chunks, settings)
    _copy_rigid_bodies(reader, chunks, settings)
    _copy_joints(reader, chunks, settings)
    if reader.remaining > 0:
        _copy_soft_bodies(reader, chunks, settings)
    if reader.remaining > 0:
        chunks.append(reader.take(reader.remaining))
    return b"".join(chunks)


def _validate_header(data: bytes) -> None:
    if len(data) < 9 or data[:4] != b"PMX ":
        raise PmxError("invalid PMX header")
    header_size = data[8]
    if header_size < 8 or len(data) < 9 + header_size:
        raise PmxError(f"invalid PMX header size: {header_size}")


def _copy_counted_fixed(reader: _Reader, chunks: list[bytes], size: int) -> None:
    count = reader.i32()
    if count < 0:
        raise PmxError("negative PMX section count")
    chunks.append(struct.pack("<i", count))
    chunks.append(reader.take(count * size))


def _convert_text(reader: _Reader, chunks: list[bytes]) -> None:
    length = reader.i32()
    source = reader.take(length)
    try:
        text = source.decode("utf-8")
    except UnicodeDecodeError as error:
        raise PmxError(f"invalid UTF-8 PMX text at byte {reader.offset - length}") from error
    encoded = text.encode("utf-16le")
    chunks.extend((struct.pack("<i", len(encoded)), encoded))


def _copy_text_section(reader: _Reader, chunks: list[bytes]) -> None:
    count = reader.i32()
    if count < 0:
        raise PmxError("negative PMX texture count")
    chunks.append(struct.pack("<i", count))
    for _ in range(count):
        _convert_text(reader, chunks)


def _copy_vertices(reader: _Reader, chunks: list[bytes], s: dict[str, int]) -> None:
    start = reader.offset
    vertex_count = reader.i32()
    if vertex_count < 0:
        raise PmxError("negative PMX vertex count")
    candidates = []
    for value in (s["boneIndexSize"], 1, 2, 4):
        if value not in candidates:
            candidates.append(value)
    last_error: Exception | None = None
    end: int | None = None
    for bone_size in candidates:
        reader.offset = start + 4
        try:
            for _ in range(vertex_count):
                _skip_vertex(reader, s, bone_size)
            if _post_vertex_plausible(reader, s):
                end = reader.offset
                break
        except (PmxError, struct.error) as error:
            last_error = error
    if end is None:
        reader.offset = start
        if last_error is not None:
            raise last_error
        raise PmxError("unable to skip PMX vertex payload")
    chunks.append(reader.data[start:end])
    reader.offset = end


def _skip_vertex(reader: _Reader, s: dict[str, int], bone_size: int) -> None:
    reader.take(12 + 12 + 8 + s["additionalUvCount"] * 16)
    weight_type = reader.u8()
    sizes = {0: bone_size, 1: bone_size * 2 + 4, 2: bone_size * 4 + 16, 3: bone_size * 2 + 4 + 36, 4: bone_size * 4 + 16}
    if weight_type not in sizes:
        raise PmxError(f"unsupported PMX vertex weight type: {weight_type}")
    reader.take(sizes[weight_type] + 4)


def _post_vertex_plausible(reader: _Reader, s: dict[str, int]) -> bool:
    if reader.offset + 4 > len(reader.data):
        return False
    count = struct.unpack_from("<i", reader.data, reader.offset)[0]
    if count < 0:
        return False
    index_bytes = count * s["vertexIndexSize"]
    if index_bytes < 0 or reader.offset + 4 + index_bytes + 4 > len(reader.data):
        return False
    texture_count = struct.unpack_from("<i", reader.data, reader.offset + 4 + index_bytes)[0]
    return texture_count >= 0


def _copy_materials(reader: _Reader, chunks: list[bytes], s: dict[str, int]) -> None:
    count = _copy_count(reader, chunks)
    for _ in range(count):
        _convert_text(reader, chunks)
        _convert_text(reader, chunks)
        chunks.append(reader.take(16 + 12 + 4 + 12 + 1 + 16 + 4))
        chunks.append(reader.take(s["textureIndexSize"]))
        chunks.append(reader.take(s["textureIndexSize"]))
        chunks.append(reader.take(1))
        toon_flag = reader.u8()
        chunks.append(bytes((toon_flag,)))
        chunks.append(reader.take(s["textureIndexSize"] if toon_flag == 0 else 1))
        _convert_text(reader, chunks)
        chunks.append(reader.take(4))


def _copy_bones(reader: _Reader, chunks: list[bytes], s: dict[str, int]) -> None:
    count = _copy_count(reader, chunks)
    for _ in range(count):
        _convert_text(reader, chunks)
        _convert_text(reader, chunks)
        chunks.append(reader.take(12 + s["boneIndexSize"] + 4))
        flags = reader.u16()
        chunks.append(struct.pack("<H", flags))
        chunks.append(reader.take(s["boneIndexSize"] if flags & 0x0001 else 12))
        if flags & 0x0100 or flags & 0x0200:
            chunks.append(reader.take(s["boneIndexSize"] + 4))
        if flags & 0x0400:
            chunks.append(reader.take(12))
        if flags & 0x0800:
            chunks.append(reader.take(24))
        if flags & 0x2000:
            chunks.append(reader.take(4))
        if flags & 0x0020:
            chunks.append(reader.take(s["boneIndexSize"] + 8))
            links = _copy_count(reader, chunks)
            for _ in range(links):
                chunks.append(reader.take(s["boneIndexSize"]))
                has_limit = reader.u8()
                chunks.append(bytes((has_limit,)))
                if has_limit:
                    chunks.append(reader.take(24))


def _copy_morphs(reader: _Reader, chunks: list[bytes], s: dict[str, int]) -> None:
    count = _copy_count(reader, chunks)
    for _ in range(count):
        _convert_text(reader, chunks)
        _convert_text(reader, chunks)
        chunks.append(reader.take(1))
        morph_type = reader.u8()
        chunks.append(bytes((morph_type,)))
        offsets = _copy_count(reader, chunks)
        for _ in range(offsets):
            sizes = {
                0: s["morphIndexSize"] + 4,
                9: s["morphIndexSize"] + 4,
                1: s["vertexIndexSize"] + 12,
                2: s["boneIndexSize"] + 28,
                8: s["materialIndexSize"] + 1 + 16 + 12 + 4 + 12 + 16 + 4 + 16 + 16 + 16,
                10: s["rigidBodyIndexSize"] + 1 + 12 + 12,
            }
            size = sizes.get(morph_type, s["vertexIndexSize"] + 16 if 3 <= morph_type <= 7 else None)
            if size is None:
                raise PmxError(f"unsupported PMX morph type: {morph_type}")
            chunks.append(reader.take(size))


def _copy_display_frames(reader: _Reader, chunks: list[bytes], s: dict[str, int]) -> None:
    count = _copy_count(reader, chunks)
    for _ in range(count):
        _convert_text(reader, chunks)
        _convert_text(reader, chunks)
        chunks.append(reader.take(1))
        items = _copy_count(reader, chunks)
        for _ in range(items):
            kind = reader.u8()
            chunks.append(bytes((kind,)))
            chunks.append(reader.take(s["boneIndexSize"] if kind == 0 else s["morphIndexSize"]))


def _copy_rigid_bodies(reader: _Reader, chunks: list[bytes], s: dict[str, int]) -> None:
    count = _copy_count(reader, chunks)
    for _ in range(count):
        _convert_text(reader, chunks)
        _convert_text(reader, chunks)
        chunks.append(reader.take(s["boneIndexSize"] + 1 + 2 + 1 + 36 + 20 + 1))


def _copy_joints(reader: _Reader, chunks: list[bytes], s: dict[str, int]) -> None:
    count = _copy_count(reader, chunks)
    for _ in range(count):
        _convert_text(reader, chunks)
        _convert_text(reader, chunks)
        chunks.append(reader.take(1 + s["rigidBodyIndexSize"] * 2 + 96))


def _copy_soft_bodies(reader: _Reader, chunks: list[bytes], s: dict[str, int]) -> None:
    count = _copy_count(reader, chunks)
    for _ in range(count):
        _convert_text(reader, chunks)
        _convert_text(reader, chunks)
        chunks.append(reader.take(1 + s["materialIndexSize"] + 1 + 2 + 1 + 4 + 4 + 4 + 4 + 4 + 48 + 24 + 16 + 12))
        anchors = _copy_count(reader, chunks)
        chunks.append(reader.take(anchors * (s["rigidBodyIndexSize"] + s["vertexIndexSize"] + 1)))
        pinned = _copy_count(reader, chunks)
        chunks.append(reader.take(pinned * s["vertexIndexSize"]))


def _copy_count(reader: _Reader, chunks: list[bytes]) -> int:
    count = reader.i32()
    if count < 0:
        raise PmxError("negative PMX section count")
    chunks.append(struct.pack("<i", count))
    return count
