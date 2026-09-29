"""Core helpers for the parametric part generators.

Everything here is deliberately small and independently testable:

  layout()   - symmetric pattern maths for rib/gap lattices
  verify()   - volume + topology assertion so a broken part can never ship
  render()   - consistent multi-view preview PNGs
  build()    - assemble the common "cut a set of slots out of a plate" case

Coordinate convention used by every generator:
    the part is authored in the XY plane and extruded along +Z,
    i.e. XY is the flat face you look at, Z is the print thickness.
"""
from __future__ import annotations

import math
from dataclasses import dataclass, field

import cadquery as cq

# ---------------------------------------------------------------- layout ----


@dataclass
class Lattice:
    """A symmetric rib/slot pattern spanning `span`.

    The pattern is  [rail][rib][gap][rib]...[gap][rib][rail]  so that every
    slot lies strictly inside the span and therefore actually breaks through
    the material instead of grazing an edge and leaving a membrane.
    """

    span: float
    rib: float
    gap: float
    n_rib: int | None = None
    rail: float = 0.0
    ribs: list[float] = field(default_factory=list)
    slots: list[float] = field(default_factory=list)

    @property
    def n_slots(self) -> int:
        return len(self.slots)

    @property
    def open_ratio(self) -> float:
        """fraction of the span that is slot (within the lattice region)."""
        if self.span <= 0:
            return 0.0
        return (self.n_slots * self.gap) / self.span


def layout(
    span: float,
    rib: float,
    gap: float,
    n_rib: int | None = None,
    min_rail: float | None = None,
) -> Lattice:
    """Fit the largest whole number of ribs into `span`, symmetrically.

    min_rail   smallest solid border allowed at each end.  Defaults to `gap`
               which is the natural companion: one slot-width of frame.
    """
    if span <= 0:
        raise ValueError(f"span must be > 0, got {span}")
    if rib <= 0 or gap < 0:
        raise ValueError(f"rib must be > 0 and gap >= 0, got {rib}, {gap}")

    min_rail = gap if min_rail is None else min_rail

    if n_rib is None:
        # how many (rib + gap) pairs fit once the two end rails are paid for?
        n_rib = int((span - 2 * min_rail) // (rib + gap))
        n_rib = max(1, n_rib)
    if n_rib < 1:
        raise ValueError(f"span {span} too small for even one {rib}mm rib")

    # shrink until it genuinely fits; the end rails are elastic, they grow to
    # absorb whatever is left over (they are never allowed to go below 0).
    while n_rib > 1 and n_rib * rib + (n_rib - 1) * gap > span:
        n_rib -= 1

    used = n_rib * rib + (n_rib - 1) * gap
    if used > span:
        raise ValueError(
            f"span {span} cannot hold {n_rib} ribs of {rib} with {gap} gaps"
        )
    rail = (span - used) / 2

    x0 = -span / 2 + rail
    lat = Lattice(span=span, rib=rib, gap=gap, n_rib=n_rib, rail=rail)
    lat.ribs = [x0 + rib / 2 + i * (rib + gap) for i in range(n_rib)]
    lat.slots = [x0 + rib + gap / 2 + i * (rib + gap) for i in range(n_rib - 1)]
    return lat


# ---------------------------------------------------------------- verify ----


class PartError(AssertionError):
    """Raised when a generated part fails its own self-check."""


def verify(
    shape: cq.Shape | cq.Workplane,
    expected_volume: float | None = None,
    *,
    tol: float = 0.005,
    require_solid: bool = True,
    label: str = "part",
) -> dict:
    """Assert the shape is a single watertight solid and, if given, that its
    volume matches the analytic expectation.  Returns a stats dict."""
    shp = shape.val() if isinstance(shape, cq.Workplane) else shape
    stats = {
        "label": label,
        "volume": shp.Volume(),
        "solids": len(shp.Solids()),
        "faces": len(shp.Faces()),
        "shells": len(shp.Shells()),
    }

    if require_solid and stats["solids"] != 1:
        raise PartError(
            f"{label}: expected 1 solid, got {stats['solids']} "
            f"-> the part is split into pieces"
        )
    if require_solid and stats["shells"] != 1:
        raise PartError(
            f"{label}: expected 1 shell (closed), got {stats['shells']} "
            f"-> the surface is not watertight"
        )

    if expected_volume is not None and expected_volume > 0:
        ratio = stats["volume"] / expected_volume
        stats["expected"] = expected_volume
        stats["ratio"] = ratio
        if abs(ratio - 1.0) > tol:
            raise PartError(
                f"{label}: volume {stats['volume']:,.1f} != expected "
                f"{expected_volume:,.1f} (ratio {ratio:.4f}, tol {tol})"
            )
        stats["ok"] = True
    return stats


def report(stats: dict) -> str:
    lines = [
        f"{stats['label']}: volume {stats['volume']:,.1f} mm^3  "
        f"faces {stats['faces']}  solids {stats['solids']}"
    ]
    if "expected" in stats:
        lines[0] += f"  (expected {stats['expected']:,.1f}, ratio {stats['ratio']:.4f})"
    if stats.get("ok"):
        lines[0] += "  OK"
    return "\n".join(lines)


# ----------------------------------------------------------------- build ----


def slotted_plate(
    lat_u: Lattice,
    lat_v: Lattice,
    length: float,
    width: float,
    thickness: float,
    *,
    through_u: bool = True,
    through_v: bool = True,
    end_rail_u: float = 0.0,
    end_rail_v: float = 0.0,
) -> tuple[cq.Workplane, dict]:
    """A flat plate with a lattice of rectangular through-slots.

    lat_u  lattice along the plate's X (length) axis; its slots run along Y
    lat_v  lattice along the plate's Y (width) axis; its slots run along X

    end_rail_u  solid margin left at each end of every u-slot, measured along
                the slot's own length (Y).  THIS is what keeps the plate in
                one piece: a slot that runs edge to edge severs the plate.
                With end_rail_u > 0 the u-slats stay joined at both ends.
    end_rail_v  the same, measured along X for the v-slots.

    Returns (Workplane, info).  info["n_slots"] is the number actually cut and
    info["slot_len_u"] / ["slot_len_v"] their finished lengths.
    """
    plate = cq.Workplane("XY").box(length, width, thickness)

    n_cut = 0
    slot_len_u = slot_len_v = 0.0

    if through_u and lat_u.n_slots:
        slot_len_u = max(0.0, width - 2 * end_rail_u)
        if slot_len_u > 0:
            cut = cq.Workplane("XY").box(lat_u.gap, slot_len_u, thickness * 2)
            for x in lat_u.slots:
                plate = plate.cut(cut.translate((x, 0, 0)))
                n_cut += 1

    if through_v and lat_v.n_slots:
        slot_len_v = max(0.0, length - 2 * end_rail_v)
        if slot_len_v > 0:
            cut = cq.Workplane("XY").box(slot_len_v, lat_v.gap, thickness * 2)
            for y in lat_v.slots:
                plate = plate.cut(cut.translate((0, y, 0)))
                n_cut += 1

    info = {
        "n_slots": n_cut,
        "slot_len_u": slot_len_u,
        "slot_len_v": slot_len_v,
        "end_rail_u": end_rail_u,
        "end_rail_v": end_rail_v,
        "lat_u": lat_u,
        "lat_v": lat_v,
    }
    return plate, info


def analytic_volume(
    length: float,
    width: float,
    thickness: float,
    lat_u: Lattice,
    lat_v: Lattice,
    through_u: bool,
    through_v: bool,
    *,
    end_rail_u: float = 0.0,
    end_rail_v: float = 0.0,
) -> float:
    """Exact material volume, by inclusion-exclusion.

    plate - all u-slots - all v-slots + (u-slot x v-slot) overlaps
    """
    v = length * width * thickness

    su_len = max(0.0, width - 2 * end_rail_u) if through_u else 0.0
    sv_len = max(0.0, length - 2 * end_rail_v) if through_v else 0.0
    n_u = len(lat_u.slots) if su_len > 0 else 0
    n_v = len(lat_v.slots) if sv_len > 0 else 0

    if n_u:
        v -= n_u * lat_u.gap * su_len * thickness
    if n_v:
        v -= n_v * lat_v.gap * sv_len * thickness
    if n_u and n_v:
        v += n_u * n_v * lat_u.gap * lat_v.gap * thickness
    return v
