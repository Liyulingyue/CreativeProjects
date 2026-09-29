"""Generate the sample parts so you can eyeball what the generators produce.

    .venv\Scripts\python.exe -m generators.demo
"""
from __future__ import annotations

import sys

from .grid_panel import grid_panel
from .render import render
from .ribbed_plate import ribbed_plate

L, W, T = 266.0, 156.0, 3.0        # the chassis panel size


def main() -> int:
    print("=== grid_panel: single-direction vent panel ===")
    for d in ("u", "v"):
        p = grid_panel(L, W, T, rib=4, gap=5, direction=d)
        print(p)
        for f in render(p.shape, f"grid_panel_{d}", views=("iso", "front", "top")):
            print("   ", f)

    print("\n=== ribbed_plate: cross lattice on a solid base ===")
    for d in ("u", "both"):
        p = ribbed_plate(L, W, base=3, rib_w=4, gap=5, rib_h=6, direction=d)
        print(p)
        for f in render(p.shape, f"ribbed_plate_{d}", views=("iso", "front", "top")):
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
