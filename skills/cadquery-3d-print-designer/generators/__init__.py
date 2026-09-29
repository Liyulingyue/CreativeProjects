"""Parametric part generators for 3D-printable enclosures.

Each generator is a small, independently testable function that takes plain
numbers and returns a verified STL-ready solid.  None of them know about the
chassis; they only know how to make one good part.

    from generators import grid_panel
    p = grid_panel(length=266, width=156, thickness=3, rib=4, gap=5)
"""
from .core import Lattice, PartError, analytic_volume, layout, report, verify
from .grid_panel import GridPanel, grid_panel
from .ribbed_plate import RibbedPlate, ribbed_plate
from .render import render

__all__ = [
    "Lattice", "PartError", "layout", "verify", "report", "analytic_volume",
    "GridPanel", "grid_panel",
    "RibbedPlate", "ribbed_plate",
    "render",
]
