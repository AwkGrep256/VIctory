# RDNA2 cube-array probe

This reproduces AwkGrep256’s Radeon 610M (`gfx1036`) measurements for VIctory issue #720. The probe builds a synthetic 4 × 4 `R32_SFLOAT` texture with two cubes and fills every texel in cube `L`, face `F` with `10 * L + F`. It runs `image_load`, `image_sample_lz`, and `image_get_resinfo` directly on the GPU. It needs no game files or VIctory build. [Hardware results in #720](https://github.com/Marvisak/VIctory/issues/720#issuecomment-7131902076)

## Run on Linux

You need Python 3.10 or later, Clang with the `amdgcn` target, GCC, the libdrm development headers, and access to an RDNA2 render node. Use a GPU with no display attached. `ls -l /dev/dri/by-path` can help identify the node. Replace the example path with yours.

```sh
export LEONIDA_HW_DRM_NODE=/dev/dri/renderD129
python3 -I cube_array_probe.py --waves 32,64 --base 0 --json measured-base0.json
python3 -I cube_array_probe.py --waves 32 --base 3 --json measured-base3.json
python3 -I cube_array_probe.py --waves 32 --base 6 --json measured-base6.json
```

Each wave and base combination runs 29 cases. The exit status is 0 when every result matches, 1 if a case fails, or 77 if a required tool or device is unavailable. `--json` saves the raw output words for each case.

## Check saved results without a GPU

From the extracted directory, run these checks against the four original `gfx1036` captures. No compiler or GPU is needed.

```sh
python3 -I cube_array_probe.py --waves 32 --check wave32.json
python3 -I cube_array_probe.py --waves 64 --check wave64.json
python3 -I cube_array_probe.py --waves 32 --base 3 --check base3.json
python3 -I cube_array_probe.py --waves 32 --base 6 --check base6.json
```

## Expected hardware behavior

`image_load` adds `BASE_ARRAY` to the input slice and returns zero beyond the last face. `image_sample_lz` decodes a face operand `f` using integer division and remainder.

```text
face = min(f % 8, 5)
first = min(BASE_ARRAY + 6 * (f // 8), DEPTH - 5)
physical slice = first + face
```

Here `DEPTH` is 11, the index of the last face. `image_get_resinfo` returns `[width, height, face count, mip levels]` for the view.

| `BASE_ARRAY` | Physical slices for operands 0–5 | Physical slices for operands 8–11 | `image_get_resinfo` |
| ---: | --- | --- | --- |
| 0 | 0–5 | 6–9 | `[4, 4, 12, 1]` |
| 3 | 3–8 | 6–9 | `[4, 4, 9, 1]` |
| 6 | 6–11 | 6–9 | `[4, 4, 6, 1]` |

Face operands 6 and 7 select face 5 within their cube. The probe uses point sampling, one mip, and face operands 0 through 15.

## Source

The bundled `hw_oracle/` files are verbatim from `tools/hw_oracle/` in Leonida commit `162821017ea3c286c8bec860f3ff94f037912061`. The harness is distributed by Leonida under GPL-3.0-or-later. [License](LICENSE)
