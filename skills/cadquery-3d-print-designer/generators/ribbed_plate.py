"""ribbed_plate - a solid plate with a lattice of ribs standing proud of it.

This is the correct primitive for a CROSS pattern.  A flat plate cut with
through-slots in both directions is geometrically many disconnected blocks no
matter what you do to the edges, so it cannot be a single printable piece.
Raising the pattern off a solid base fixes that: the base ties every rib
together, the result is one solid, and the channels between the ribs still
let air through.

    from generators import ribbed_plate
    p = ribbed_plate(length=266, width=156, base=3, rib_w=4, gap=5, rib_h=6)

Direction semantics
-------------------
ribs_u   ribs run along Y (long axis is Y), repeated along X
ribs_v   ribs run along X, repeated along Y
Both on  = a cross lattice standing on the base, joined by `cross_ribs`
"""
from __future__ import annotations

from dataclasses import dataclass, field

import cadquery as cq

from .core import Lattice, PartError, layout, report, verify


@dataclass
class RibbedPlate:
    shape: cq.Workplane
    stats: dict
    lat_u: Lattice
    lat_v: Lattice
    params: dict = field(default_factory=dict)

    def save(self, path) -> None:
        cq.exporters.export(self.shape, str(path))

    @property
    def open_ratio(self) -> float:
        """channel area / face area in the rib layer."""
        p = self.params
        if p["rib_h"] <= 0 or p["length"] <= 0 or p["width"] <= 0:
            return 0.0
        cell = p["rib_w"] + p["gap"]
        # fraction of the face not covered by rib material, both directions
        return (1 - p["rib_w"] / cell) if p["direction"] != "both" else \
            (1 - p["rib_w"] / cell) ** 2

    def __str__(self) -> str:
        p = self.params
        return (
            f"{self.stats['label']}  {p['length']}x{p['width']}  "
            f"base {p['base']} + rib {p['rib_h']}  "
            f"rib {p['rib_w']} gap {p['gap']}  "
            f"ribs {self.lat_u.n_rib}/{self.lat_v.n_rib}  "
            f"open {self.open_ratio*100:.0f}%\n{report(self.stats)}"
        )


def ribbed_plate(
    length: float,
    width: float,
    base: float = 3.0,
    rib_w: float = 4.0,
    gap: float = 5.0,
    rib_h: float = 6.0,
    *,
    direction: str = "u",
    # 'u' ribs run along Y and repeat along X
    # 'v' ribs run along X and repeat along Y
    # 'both' a cross lattice, with `cross_ribs` of them running the other way
    #       and tying the two families together
    n_rib_u: int | None = None,
    n_rib_v: int | None = None,
    end_margin: float = 0.0,
    fillet: float = 0.0,
    label: str = "ribbed_plate",
) -> RibbedPlate:
    """Solid base plate with a lattice of ribs standing on it.

    Always one connected solid: every rib sits on the base, so the base is the
    tie that holds the lattice together.
    """
    if direction not in ("u", "v", "both"):
        raise ValueError(f"direction must be u|v|both, got {direction!r}")
    if base <= 0:
        raise ValueError(f"base must be > 0, got {base}")
    if rib_h <= 0:
        raise ValueError(f"rib_h must be > 0, got {rib_h}")
    if length <= 0 or width <= 0:
        raise ValueError("length and width must be > 0")

    # `.box()` is centred on the origin, so the base spans -base/2..+base/2
    # and its TOP surface sits at z = base/2.  Ribs must start there, not at
    # z = base, or they float clear of the base and the part falls apart.
    base_top = base / 2
    shape = cq.Workplane("XY").box(length, width, base)
    total_h = base + rib_h

    lat_u = Lattice(1, rib_w, 0, 1)
    lat_v = Lattice(1, rib_w, 0, 1)
    lat_u.slots, lat_v.slots = [], []

    hu = width - 2 * end_margin
    hv = length - 2 * end_margin

    if direction in ("u", "both"):
        lat_u = layout(length, rib_w, gap, n_rib=n_rib_u)
        for x in lat_u.ribs:
            shape = shape.union(
                cq.Workplane("XY")
                .box(rib_w, hu, rib_h)
                .translate((x, 0, base_top + rib_h / 2))
            )

    if direction in ("v", "both"):
        lat_v = layout(width, rib_w, gap, n_rib=n_rib_v)
        for y in lat_v.ribs:
            shape = shape.union(
                cq.Workplane("XY")
                .box(hv, rib_w, rib_h)
                .translate((0, y, base_top + rib_h / 2))
            )

    if fillet > 0:
        try:
            shape = shape.edges("|Z").fillet(fillet)
        except Exception:
            pass

    # ---- analytic volume -----------------------------------------------
    # base slab + each rib prism - the square where a u-rib crosses a v-rib,
    # since that material was counted twice.
    v = length * width * base
    n_u = lat_u.n_rib if direction in ("u", "both") else 0
    n_v = lat_v.n_rib if direction in ("v", "both") else 0
    if n_u:
        v += n_u * rib_w * hu * rib_h
    if n_v:
        v += n_v * hv * rib_w * rib_h
    if n_u and n_v:
        v -= n_u * n_v * rib_w * rib_w * rib_h

    stats = verify(shape, v, label=label)
    params = dict(
        length=length, width=width, base=base, rib_h=rib_h,
        rib_w=rib_w, gap=gap, direction=direction,
        n_rib_u=n_u, n_rib_v=n_v, total_h=total_h,
    )
    return RibbedPlate(shape, stats, lat_u, lat_v, params)
