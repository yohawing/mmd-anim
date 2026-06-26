"""Dump FBX binary node tree structure for debugging.

Usage: python scripts/dump_fbx_nodes.py <file.fbx> [--max-depth N]
"""

import struct
import sys
from pathlib import Path


def read_property(f):
    type_code = f.read(1)
    if not type_code:
        return None, None
    tc = chr(type_code[0])
    if tc == "Y":
        return tc, struct.unpack("<h", f.read(2))[0]
    elif tc == "C":
        return tc, struct.unpack("<B", f.read(1))[0]
    elif tc == "I":
        return tc, struct.unpack("<i", f.read(4))[0]
    elif tc == "F":
        return tc, struct.unpack("<f", f.read(4))[0]
    elif tc == "D":
        return tc, struct.unpack("<d", f.read(8))[0]
    elif tc == "L":
        return tc, struct.unpack("<q", f.read(8))[0]
    elif tc == "S":
        length = struct.unpack("<I", f.read(4))[0]
        return tc, f.read(length)
    elif tc == "R":
        length = struct.unpack("<I", f.read(4))[0]
        f.read(length)
        return tc, f"<{length} bytes>"
    elif tc in ("f", "d", "l", "i", "b"):
        count, encoding, comp_len = struct.unpack("<III", f.read(12))
        f.read(comp_len)
        names = {"f": "f32", "d": "f64", "l": "i64", "i": "i32", "b": "bool"}
        return tc, f"[{names.get(tc, tc)}; {count}]"
    else:
        return tc, f"<unknown {tc}>"


def fmt(tc, val):
    if tc == "S" and isinstance(val, bytes):
        try:
            d = val.decode("utf-8", errors="replace")
            if "\x00\x01" in d:
                parts = d.split("\x00\x01")
                return repr(parts[0]) + " {" + parts[-1] + "}"
            return repr(d)
        except Exception:
            return repr(val)
    return repr(val)


def dump_nodes(f, end_offset, use64, depth=0, max_depth=4):
    while f.tell() < end_offset:
        pos = f.tell()
        if use64:
            header = f.read(25)
            if len(header) < 25:
                break
            child_end, num_props, prop_len = struct.unpack("<QQQ", header[:24])
            name_len = header[24]
        else:
            header = f.read(13)
            if len(header) < 13:
                break
            child_end, num_props, prop_len = struct.unpack("<III", header[:12])
            name_len = header[12]

        if child_end == 0 and num_props == 0 and prop_len == 0 and name_len == 0:
            break

        name = f.read(name_len).decode("ascii", errors="replace")

        prop_start = f.tell()
        props = []
        for _ in range(num_props):
            tc, val = read_property(f)
            if tc:
                props.append((tc, val))

        indent = "  " * depth
        ps = [f"{tc}:{fmt(tc, v)}" for tc, v in props[:5]]
        summary = ", ".join(ps)
        if len(props) > 5:
            summary += f" +{len(props)-5}"
        print(f"{indent}{name} [{num_props}p] {summary}")

        if f.tell() < child_end:
            if depth < max_depth:
                dump_nodes(f, child_end, use64, depth + 1, max_depth)
            else:
                print(f"{indent}  ...")
                f.seek(child_end)


def main():
    args = sys.argv[1:]
    if not args:
        print(__doc__.strip())
        sys.exit(1)

    fbx_path = args[0]
    max_depth = int(args[args.index("--max-depth") + 1]) if "--max-depth" in args else 4

    with open(fbx_path, "rb") as f:
        magic = f.read(23)
        if not magic.startswith(b"Kaydara FBX Binary"):
            print(f"Not FBX binary: {fbx_path}")
            sys.exit(1)
        version = struct.unpack("<I", f.read(4))[0]
        use64 = version >= 7500
        print(f"FBX v{version} ({'64-bit' if use64 else '32-bit'} offsets) - {Path(fbx_path).name}\n")
        dump_nodes(f, Path(fbx_path).stat().st_size, use64, max_depth=max_depth)


if __name__ == "__main__":
    main()
