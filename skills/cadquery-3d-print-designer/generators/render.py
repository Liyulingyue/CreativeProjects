"""Preview renderer shared by all generators.

    from generators import grid_panel, render
    p = grid_panel(266, 156, 3, rib=4, gap=5, direction="u")
    render(p, "panel", views=("iso", "front", "top"), outdir="out")

`render()` accepts anything that can be turned into geometry: a generator
result (GridPanel / RibbedPlate), a cq.Workplane or a cq.Shape.  The
generator dataclasses carry their solid in a `.shape` attribute and are
unwrapped automatically, so you never have to remember to pass `p.shape`.

Edges
-----
By default no edges are drawn.  On a tessellated STL, PyVista's
`show_edges=True` draws the *triangle* boundaries, not the part's feature
edges, so a flat panel comes out looking like a mesh of little squares
instead of a row of slots.  To get the real feature edges - the ones a
slicer would care about - ask for them by angle:

    render(p, "panel", edges=30)      # edges sharper than 30 degrees

Anything above ~25 degrees hides the triangulation (whose dihedral angle is
0) and keeps only real corners.
"""
from __future__ import annotations

from pathlib import Path

import cadquery as cq
import pyvista as pv

VIEWS = {
    "iso": "view_isometric",
    "front": "view_xz",
    "side": "view_yz",
    "top": "view_xy",
    "edge": "view_yx",
}


def _shape_of(obj) -> cq.Shape | cq.Workplane:
    """Unwrap a generator result, Workplane or Shape into raw geometry."""
    inner = getattr(obj, "shape", None)
    if isinstance(inner, (cq.Workplane, cq.Shape)):
        return inner
    if isinstance(obj, (cq.Workplane, cq.Shape)):
        return obj
    raise TypeError(
        f"cannot render {type(obj).__name__}; expected a GridPanel / "
        f"RibbedPlate / cq.Workplane / cq.Shape"
    )


def render(
    shape,
    stem: str,
    views: tuple[str, ...] = ("iso", "front", "top"),
    zoom: float = 1.0,
    extra=None,
    outdir: str | Path = "out",
    *,
    edges: bool | float = False,
    bounds: bool = False,
    background: str = "white",
    color: str = "lightgray",
    window_size: tuple[int, int] = (1300, 950),
) -> list[Path]:
    """Write <stem>_<view>.png for each requested view and return the paths.

    shape   a generator result, cq.Workplane or cq.Shape
    views   any of VIEWS' keys
    extra   an optional second shape, drawn translucent as a reference
    edges   False/True, or a dihedral angle in degrees.  A number draws only
            feature edges sharper than that angle (use ~30 to drop the STL
            triangulation); True draws every edge including triangles.
    bounds  draw the bounding box and the X/Y/Z axes
    """
    outdir = Path(outdir)
    outdir.mkdir(parents=True, exist_ok=True)
    stl = outdir / f"{stem}.stl"
    cq.exporters.export(_shape_of(shape), str(stl))

    mesh = pv.read(str(stl))
    extra_m = None
    if extra is not None:
        cq.exporters.export(_shape_of(extra), str(outdir / f"{stem}_ref.stl"))
        extra_m = pv.read(str(outdir / f"{stem}_ref.stl"))

    if edges is False:
        edge_kw: dict = {"show_edges": False}
    elif edges is True:
        edge_kw = {"show_edges": True, "edge_color": "dimgray"}
    else:
        edge_kw = {
            "show_edges": True,
            "edge_color": "dimgray",
            "feature_angle": float(edges),
        }

    written = [stl]
    for v in views:
        pl = pv.Plotter(off_screen=True, window_size=list(window_size))
        pl.set_background(background)
        pl.add_mesh(mesh, color=color, specular=0.3, **edge_kw)
        if extra_m is not None:
            pl.add_mesh(extra_m, color="dodgerblue", opacity=0.16,
                        show_edges=True, edge_color="steelblue", specular=0.0)
        if bounds:
            pl.add_bounding_box(color="black", line_width=1)
            pl.show_bounds(grid="back", location="outer", ticks="both",
                           xtitle="X (mm)", ytitle="Y (mm)", ztitle="Z (mm)")
        pl.enable_parallel_projection()
        getattr(pl, VIEWS[v])()
        pl.camera.zoom(zoom)
        p = outdir / f"{stem}_{v}.png"
        pl.screenshot(str(p))
        pl.close()
        written.append(p)
    return written
