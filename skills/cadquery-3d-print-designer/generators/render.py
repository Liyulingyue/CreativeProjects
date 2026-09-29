"""Preview renderer shared by all generators."""
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


def render(
    shape: cq.Shape,
    stem: str,
    views: tuple[str, ...] = ("iso", "front", "top"),
    zoom: float = 1.0,
    extra: cq.Shape | None = None,
    outdir: str | Path = "out",
) -> list[Path]:
    """Write <stem>_<view>.png for each requested view and return the paths."""
    outdir = Path(outdir)
    outdir.mkdir(parents=True, exist_ok=True)
    stl = outdir / f"{stem}.stl"
    cq.exporters.export(shape, str(stl))

    mesh = pv.read(str(stl))
    extra_m = None
    if extra is not None:
        cq.exporters.export(extra, str(outdir / f"{stem}_ref.stl"))
        extra_m = pv.read(str(outdir / f"{stem}_ref.stl"))

    written = [stl]
    for v in views:
        pl = pv.Plotter(off_screen=True, window_size=[1300, 950])
        pl.set_background("white")
        pl.add_mesh(mesh, color="lightgray", show_edges=True,
                    edge_color="dimgray", specular=0.3)
        if extra_m is not None:
            pl.add_mesh(extra_m, color="dodgerblue", opacity=0.16,
                        show_edges=True, edge_color="steelblue", specular=0.0)
        pl.enable_parallel_projection()
        getattr(pl, VIEWS[v])()
        pl.camera.zoom(zoom)
        p = outdir / f"{stem}_{v}.png"
        pl.screenshot(str(p))
        pl.close()
        written.append(p)
    return written
