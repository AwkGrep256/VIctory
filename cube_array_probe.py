"""Measure cube-array addressing on RDNA2 for VIctory #720.

The texture is a cube array of 2 cubes (12 faces), R32_SFLOAT, linear, 4x4 texels per face. Every texel of
layer L, face F holds the float 10 * L + F. One kernel per case, one lane each:

  load   image_load dim:CUBE at (1, 1, slice): checks the layout (slice = 6 * L + F is the usual rule)
  sample image_sample_lz dim:CUBE at s = t = 1.5 with face operand f: check 8 * layer + face and clamping
  resinfo image_get_resinfo dim:CUBE, mip 0: width, height, array size, levels

Run a hardware test on Linux or check a saved JSON report on any platform. See README.md.
"""
import argparse
import json
import struct
import sys
from pathlib import Path

sys.dont_write_bytecode = True
sys.path.insert(0, str(Path(__file__).resolve().parent))

from hw_oracle import asm, runner, tsharp  # noqa: E402

CUBE = 11                 # SQ_RSRC_IMG_CUBE (WORD3 TYPE)
W = H = 4                 # texels per face edge
CUBES = 2
SLICES = 6 * CUBES
SENTINEL = 0xDEAD0000
POISON = 0xBAADF00D


def f2u(f):
    return struct.unpack("<I", struct.pack("<f", f))[0]


def value(slice_):
    return 10 * (slice_ // 6) + slice_ % 6


def texture():
    """12 slices of H rows; each row padded to 256 bytes (the default linear pitch), every texel = value(slice)."""
    pitch = tsharp.pitch_texels(W, 4)
    out = bytearray()
    for s in range(SLICES):
        row = struct.pack("<f", float(value(s))) * W + b"\xcd" * ((pitch - W) * 4)
        out += row * H
    return bytes(out)


def tsharp_words(arch, base=0):
    """tsharp's linear 2D T# turned into a cube array: TYPE CUBE, DEPTH = last layer, BASE_ARRAY = first face.
    WORD4 drops the custom pitch tsharp puts there, so the default pitch (256-byte rows) applies."""
    t = tsharp.image_words(arch, "R32_SFLOAT", W, H, 4)
    t[3] = (t[3] & ~(0xF << 28)) | (CUBE << 28)
    t[4] = ((SLICES - 1) & 0x1FFF) | (base << 16)     # BASE_ARRAY [28:16]: the view's first face
    return t


def kernel(arch, uid, op, coords, wave, base=0):
    """One lane: build the T# around the texture embedded after s_endpgm, run `op` with v[2:4] = coords (floats or
    ints as given), store v[8:11]."""
    t = tsharp_words(arch, base)
    lines = ["s_getpc_b64 s[4:5]",
             f".Lpc{uid}:",
             f"s_add_u32 s4, s4, .Ltex{uid}-.Lpc{uid}",
             "s_addc_u32 s5, s5, 0",
             "s_and_b32 s6, s4, 255",
             "s_lshr_b64 s[4:5], s[4:5], 8",
             "s_mov_b32 s8, s4",
             "s_and_b32 s9, s5, 255",
             f"s_or_b32 s9, s9, 0x{t[1]:08x}"]
    lines += [f"s_mov_b32 s{8 + i}, 0x{t[i]:08x}" for i in range(2, 8)]
    lines += [f"s_mov_b32 s{16 + i}, 0" for i in range(4)]   # S#: wrap, point filters, no mips
    lines += [f"v_mov_b32 v{8 + i}, 0x{SENTINEL + i:08x}" for i in range(4)]
    lines += [f"v_mov_b32 v{2 + i}, 0x{c:08x}" for i, c in enumerate(coords)]
    lines += ["s_cmp_lg_u32 s6, 0", f"s_cbranch_scc0 .Lok{uid}"]
    lines += [f"v_mov_b32 v{8 + i}, 0x{POISON:08x}" for i in range(4)]
    lines += [f"s_branch .Lst{uid}", f".Lok{uid}:", op, "s_waitcnt vmcnt(0)", f".Lst{uid}:",
              "v_mov_b32 v4, 0", "global_store_b128 v4, v[8:11], s[2:3]", "s_endpgm",
              ".p2align 8", f".Ltex{uid}:"]
    lines += tsharp._longs(texture())
    return asm.Kernel(lines, wave=wave, threads=1, words=4, raw=True)


def cases():
    out = []
    for s in range(SLICES):
        out.append(("load", s, "image_load v8, v[2:4], s[8:15] dmask:0x1 dim:SQ_RSRC_IMG_CUBE unorm", (1, 1, s)))
    for f in range(16):
        out.append(("sample", f, "image_sample_lz v8, v[2:4], s[8:15], s[16:19] dmask:0x1 dim:SQ_RSRC_IMG_CUBE",
                    (f2u(1.5), f2u(1.5), f2u(float(f)))))
    out.append(("resinfo", 0, "image_get_resinfo v[8:11], v2, s[8:15] dmask:0xf dim:SQ_RSRC_IMG_CUBE unorm",
                (0, 0, 0)))
    return out


def expected_words(kind, arg, base):
    if kind == "resinfo":
        return [W, H, SLICES - base, 1]
    if kind == "load":
        slice_ = base + arg
        texel = value(slice_) if slice_ < SLICES else 0
    else:
        layer, face = divmod(arg, 8)
        first = min(base + 6 * layer, SLICES - 6)
        texel = value(first + min(face, 5))
    return [f2u(texel), SENTINEL + 1, SENTINEL + 2, SENTINEL + 3]


def check_report(report, waves, base):
    if report.get("base", base) != base:
        print(f"Report BASE_ARRAY is {report['base']}, expected {base}", file=sys.stderr)
        return 1

    expected = {
        (wave, kind, arg): expected_words(kind, arg, base)
        for wave in waves
        for kind, arg, _, _ in cases()
    }
    passed = 0
    total = len(expected)
    errors = 0
    for row in report["results"]:
        key = (row["wave"], row["kind"], row["arg"])
        want = expected.pop(key, None)
        if want is None:
            print(f"{key}: unexpected or duplicate result", file=sys.stderr)
            errors += 1
        elif "error" in row or row.get("got") != want:
            print(f"{key}: expected {want}, got {row.get('error', row.get('got'))}", file=sys.stderr)
            errors += 1
        else:
            passed += 1
    for key in expected:
        print(f"{key}: missing result", file=sys.stderr)
    print(f"{report['arch']}, BASE_ARRAY={base}: {passed}/{total} cases matched")
    return int(bool(errors or expected))


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    parser.add_argument("--waves", choices=("32", "64", "32,64"), default="32,64")
    parser.add_argument("--base", type=int, choices=range(SLICES - 5), default=0,
                        help="First face of the view (default: 0)")
    mode = parser.add_mutually_exclusive_group()
    mode.add_argument("--json", type=Path, help="Save hardware results to this file")
    mode.add_argument("--check", type=Path, help="Check an existing report without a GPU")
    args = parser.parse_args(argv)
    waves = [int(wave) for wave in args.waves.split(",")]
    if args.check:
        try:
            return check_report(json.loads(args.check.read_text(encoding="utf-8")), waves, args.base)
        except (OSError, ValueError, KeyError, TypeError, AttributeError) as error:
            parser.exit(1, f"Cannot read {args.check}: {error}\n")

    if not sys.platform.startswith("linux"):
        print("Hardware mode requires Linux and an RDNA2 render node", file=sys.stderr)
        return 77
    try:
        oracle = runner.open_oracle()
    except runner.Unavailable as e:
        print(f"Hardware oracle unavailable: {e}", file=sys.stderr)
        return 77
    report = {"arch": oracle.arch, "base": args.base, "results": []}
    try:
        if not oracle.arch.startswith("gfx103"):
            print(f"Hardware mode requires RDNA2 (gfx103x), found {oracle.arch}", file=sys.stderr)
            return 77
        probe_cases = cases()
        for wave in waves:
            kernels = [kernel(oracle.arch, f"{wave}_{i}", op, coords, wave, args.base)
                       for i, (_, _, op, coords) in enumerate(probe_cases)]
            for (kind, arg, _, _), got in zip(probe_cases, oracle.run(kernels), strict=True):
                row = {"wave": wave, "kind": kind, "arg": arg}
                if isinstance(got, str):
                    row["error"] = got
                elif POISON in got:
                    row["error"] = "texture misaligned"
                else:
                    row["got"] = got
                report["results"].append(row)
    finally:
        oracle.close()
    if args.json:
        args.json.write_text(json.dumps(report, indent=1) + "\n", encoding="utf-8")
    return check_report(report, waves, args.base)


if __name__ == "__main__":
    sys.exit(main())
