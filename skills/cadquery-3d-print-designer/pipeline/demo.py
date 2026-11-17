r"""Generate the sample parts so you can eyeball what the generators produce.

    .venv/bin/python -m pipeline.demo
    .venv\Scripts\python.exe -m pipeline.demo
"""
from __future__ import annotations

import sys

from examples.blade import BladeSpec, table_tennis_blade
from examples.grid_panel import grid_panel
from examples.hanger_hook import hanger_hook, hanger_hook_cap
from examples.paddle import PaddleSpec, paddle_assembly
from .render import render
from examples.ribbed_plate import ribbed_plate

L, W, T = 266.0, 156.0, 3.0        # the chassis panel size


def main() -> int:
    print("=== grid_panel: single-direction vent panel ===")
    for d in ("u", "v"):
        p = grid_panel(L, W, T, rib=4, gap=5, direction=d)
        print(p)
        for f in render(p, f"grid_panel_{d}", views=("iso", "front", "top")):
            print("   ", f)

    print("\n=== ribbed_plate: cross lattice on a solid base ===")
    for d in ("u", "both"):
        p = ribbed_plate(L, W, base=3, rib_w=4, gap=5, rib_h=6, direction=d)
        print(p)
        for f in render(p, f"ribbed_plate_{d}", views=("iso", "front", "top")):
            print("   ", f)

    print("\n=== one dimensioned view, with axes ===")
    p = grid_panel(L, W, T, rib=4, gap=5, direction="u")
    for f in render(p, "grid_panel_u_measured", views=("iso",),
                    edges=30, bounds=True):
        print("   ", f)

    print("\n=== hanger hook tip cap, on a reference hook ===")
    cap = hanger_hook_cap()
    hook = hanger_hook()
    print(cap)
    for f in render(cap, "hanger_hook_cap", views=("iso", "side", "top"),
                    extra=hook.shape, edges=30):
        print("   ", f)

    print("\n=== one-piece table tennis blade ===")
    blade = table_tennis_blade(BladeSpec(edge_step=1.0))
    print(blade)
    for f in render(blade, "table_tennis_blade",
                    views=("top", "iso", "front"), edges=30, outdir="out/blade"):
        print("   ", f)

    print("\n=== quick-release table tennis paddle ===")
    # the paddle gets its own folder so it does not drown the other parts
    paddle_out = "out/paddle"
    spec = PaddleSpec()
    parts = paddle_assembly(spec)
    for name, part in parts.items():
        print(part)
    for name, part in parts.items():
        if name == "m3_screw":
            continue
        for f in render(part, name, views=("top", "iso"), edges=30,
                        outdir=paddle_out):
            print("   ", f)

    print("\n=== the paddle assembled, and cut open at the dovetail ===")
    asm = parts["blade_half_1"].shape
    for name in ("blade_half_2", "handle_clamp_1", "handle_clamp_2"):
        asm = asm.union(parts[name].shape)
    asm = asm.union(parts["rubber_sheet"].shape)
    asm = asm.union(
        parts["rubber_sheet"].shape.translate(
            (0, 0, -spec.blade_t - spec.rubber_t)
        )
    )
    asm = asm.union(parts["m3_screw"].shape)
    asm = asm.union(parts["m3_screw"].shape.translate((-2 * spec.screw_x, 0, 0)))
    for f in render(asm, "assembly", views=("top", "iso", "front"), edges=30,
                    outdir=paddle_out):
        print("   ", f)

    import cadquery as cq

    neck = asm.intersect(
        cq.Workplane("XY").box(200, 60, 200).translate((0, -20, 0))
    )
    cut = cq.Workplane("XY").box(200, 200, 200).translate((0, 90, 0))
    for f in render(neck.cut(cut), "neck_section",
                    views=("front", "iso"), edges=30, zoom=1.6,
                    outdir=paddle_out):
        print("   ", f)

    print("\n=== parameter sweep sanity (no render, just the numbers) ===")
    print(f"{'gap':>5} {'ribs':>5} {'slots':>6} {'open%':>7} {'volume mm3':>12}")
    for gap in (4, 5, 6, 8, 10, 12):
        p = grid_panel(L, W, T, rib=4, gap=gap, direction="u")
        print(f"{gap:>5} {p.lat_u.n_rib:>5} {p.params['n_slots']:>6} "
              f"{p.open_ratio*100:>6.1f}% {p.stats['volume']:>12,.0f}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
