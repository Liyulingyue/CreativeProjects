"""grid_panel - a flat plate perforated by a lattice of rectangular slots.

The workhorse part: chassis walls, floors, lids, fan guards, filter frames.
Give it a rectangle and a rib/gap pitch and it returns a watertight,
slice-ready, *single-piece* solid.

    from generators import grid_panel
    p = grid_panel(length=266, width=156, thickness=3, rib=4, gap=5)
    p.save("wall.stl")
    print(p)

Design note - why end_rail matters
----------------------------------
A slot that runs edge to edge across a flat plate severs it.  With a both-axis
lattice on a 266x156 plate, cutting every slot full length splits the part into
hundreds of loose blocks that no slicer can print.  end_rail shortens each slot
at both ends, leaving solid rails that tie the lattice together, so the panel
stays one solid while air still passes straight through.
"""
from __future__ import annotations

from dataclasses import dataclass, field

import cadquery as cq

from .core import (
    Lattice,
    PartError,
    analytic_volume,
    layout,
    report,
    slotted_plate,
    verify,
)


@dataclass
class GridPanel:
    shape: cq.Workplane
    stats: dict
    lat_u: Lattice
    lat_v: Lattice
    params: dict = field(default_factory=dict)

    def save(self, path) -> None:
        cq.exporters.export(self.shape, str(path))

    def __str__(self) -> str:
        p = self.params
        return (
            f"{self.stats['label']}  {p['length']}x{p['width']}x{p['thickness']}  "
            f"rib {p['rib']} gap {p['gap']}  "
            f"ribs {self.lat_u.n_rib}x{self.lat_v.n_rib}  slots {p['n_slots']}  "
            f"open {self.open_ratio*100:.0f}%\n{report(self.stats)}"
        )

    @property
    def open_ratio(self) -> float:
        """void area / face area, which is what actually governs airflow."""
        p = self.params
        face = p["length"] * p["width"]
        return 1.0 - self.stats["volume"] / (face * p["thickness"])


def grid_panel(
    length: float,
    width: float,
    thickness: float,
    rib: float = 4.0,
    gap: float = 5.0,
    *,
    # --- pattern control -------------------------------------------------
    # direction: 'u'   slots thin along X, long along Y
    #            'v'   slots thin along Y, long along X
    #            'both' cross lattice (default)
    #            'none' solid plate, a blank
    direction: str = "both",
    n_rib_u: int | None = None,
    n_rib_v: int | None = None,
    min_rail: float | None = None,

    # --- connectivity ----------------------------------------------------
    # end_rail_u: solid margin at both ends of every u-slot, along Y
    # end_rail_v: solid margin at both ends of every v-slot, along X
    # Set these > 0 (or pass auto_rails=True) or a both-axis panel will not be
    # a single solid.
    end_rail_u: float = 0.0,
    end_rail_v: float = 0.0,
    auto_rails: bool = True,
    allow_disconnected: bool = False,

    # --- cosmetic --------------------------------------------------------
    # A solid margin around the lattice is just a bigger end rail, so use
    # end_rail_u / end_rail_v for that rather than a separate border knob.
    fillet: float = 0.0,
    label: str = "grid_panel",
) -> GridPanel:
    """Build a perforated plate.

    Raises PartError if the result is not one watertight solid or if its
    volume disagrees with the analytic value, so a broken panel can never be
    silently written to disk.
    """
    if direction not in ("u", "v", "both", "none"):
        raise ValueError(f"direction must be u|v|both|none, got {direction!r}")
    if thickness <= 0:
        raise ValueError(f"thickness must be > 0, got {thickness}")
    if direction == "both" and end_rail_u <= 0 and end_rail_v <= 0 \
            and not auto_rails and not allow_disconnected:
        raise PartError(
            "direction='both' on a flat plate is geometrically many separate "
            "blocks - no edge treatment can keep it one piece. Use "
            "ribbed_plate() for a cross pattern, or set allow_disconnected=True "
            "if you really want loose blocks."
        )

    lat_u = (
        layout(length, rib, gap, n_rib=n_rib_u, min_rail=min_rail)
        if direction in ("u", "both")
        else layout(length, rib, 0.0, n_rib=1)
    )
    lat_v = (
        layout(width, rib, gap, n_rib=n_rib_v, min_rail=min_rail)
        if direction in ("v", "both")
        else layout(width, rib, 0.0, n_rib=1)
    )
    through_u = direction in ("u", "both")
    through_v = direction in ("v", "both")

    # ---- keep it in one piece -------------------------------------------
    if auto_rails and through_u and through_v:
        if end_rail_u <= 0:
            end_rail_u = max(rib, gap)
        if end_rail_v <= 0:
            end_rail_v = max(rib, gap)
    if auto_rails and through_u and not through_v:
        end_rail_u = max(rib, gap) if end_rail_u <= 0 else end_rail_u
    if auto_rails and through_v and not through_u:
        end_rail_v = max(rib, gap) if end_rail_v <= 0 else end_rail_v

    if (through_u and through_v and not allow_disconnected):
        raise PartError(
            f"grid_panel(direction='both') cannot be one solid: on a flat "
            f"plate, slots running in two directions box the interior into "
            f"separate blocks. Use ribbed_plate(direction='both') for a cross "
            f"pattern, or a single direction here."
        )

    shape, info = slotted_plate(
        lat_u, lat_v, length, width, thickness,
        through_u=through_u, through_v=through_v,
        end_rail_u=end_rail_u, end_rail_v=end_rail_v,
    )

    if fillet > 0:
        try:
            shape = shape.edges("|Z").fillet(fillet)
        except Exception:                    # cosmetic only, never fatal
            pass

    # ---- verify ---------------------------------------------------------
    expected = analytic_volume(
        length, width, thickness, lat_u, lat_v,
        through_u, through_v,
        end_rail_u=end_rail_u, end_rail_v=end_rail_v,
    )

    stats = verify(
        shape, expected, label=label,
        require_solid=not (through_u and through_v and allow_disconnected),
    )

    params = dict(
        length=length, width=width, thickness=thickness,
        rib=rib, gap=gap, direction=direction,
        n_slots=info["n_slots"],
        end_rail_u=info["end_rail_u"], end_rail_v=info["end_rail_v"],
    )
    return GridPanel(shape, stats, lat_u, lat_v, params)
