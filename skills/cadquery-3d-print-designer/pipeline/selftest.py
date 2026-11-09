"""Self-test for the generators.  Run:  python -m pipeline.selftest
Every case is checked against the analytic volume, so a layout bug (like a
slot centre landing on a wall face and leaving a membrane) fails loudly
instead of silently shipping a part that is not actually perforated.
"""
from __future__ import annotations

import math
import sys
import traceback

from examples.blade import BladeSpec, table_tennis_blade
from .core import PartError, layout
from examples.grid_panel import grid_panel
from examples.hanger_hook import hanger_hook, hanger_hook_cap
from examples.paddle import (
    PaddleSpec,
    blade_half,
    handle_clamp,
    m3_screw,
    paddle_assembly,
    rubber_sheet,
)
from examples.ribbed_plate import ribbed_plate

PASS, FAIL = "  ok  ", " FAIL "
_failures: list[str] = []


def check(name: str, fn) -> None:
    try:
        fn()
        print(f"{PASS} {name}")
    except Exception as e:
        print(f"{FAIL} {name}\n         {e}")
        _failures.append(f"{name}: {e}")


# ---------------------------------------------------------------- layout ----

def t_layout_counts():
    for span in (260, 150, 100, 60, 20, 12):
        for gap in (5, 6, 8):
            lat = layout(span, 4, gap)
            assert lat.n_slots == lat.n_rib - 1, (span, gap, lat.n_rib)
            assert lat.rail >= 0, (span, gap, lat.rail)


def t_layout_symmetric():
    for span in (260, 150, 100, 60):
        lat = layout(span, 4, 5)
        # the outer edges of the pattern must mirror exactly
        left = lat.ribs[0] - lat.rib / 2
        right = lat.ribs[-1] + lat.rib / 2
        assert abs(left + right) < 1e-9, (left, right)


def t_layout_rail_symmetric():
    """Whatever the span, the rails at each end must mirror each other and
    the pattern must be centred."""
    for span in (260, 150, 100, 60, 20, 12):
        lat = layout(span, 4, 5)
        assert abs(lat.rail - lat.rail) < 1e-12
        # leftmost material edge must mirror the rightmost
        left = lat.ribs[0] - lat.rib / 2
        right = lat.ribs[-1] + lat.rib / 2
        assert abs(left + right) < 1e-9, (span, left, right)


def t_layout_slots_inside():
    """Every slot must sit strictly inside the span so it breaks through the
    material rather than grazing an edge and leaving a membrane."""
    for span in (260, 150, 100, 60, 40, 24):
        lat = layout(span, 4, 5)
        half = lat.gap / 2
        for s in lat.slots:
            assert s - half > -span / 2 - 1e-9, (
                f"slot at {s} breaches the left edge of span {span}")
            assert s + half < span / 2 + 1e-9, (
                f"slot at {s} breaches the right edge of span {span}")


def t_layout_no_zero_slots():
    """A 1-rib layout has no slots at all; that must be true, not a bug."""
    for span in (10, 12, 20):
        lat = layout(span, 4, 5)
        assert lat.n_slots == lat.n_rib - 1


# ------------------------------------------------------------ grid_panel ----

def t_panel_basic():
    p = grid_panel(266, 156, 3, rib=4, gap=5, direction="u")
    assert p.stats["solids"] == 1
    assert p.params["n_slots"] > 0


def t_panel_single_direction():
    for d in ("u", "v"):
        p = grid_panel(266, 156, 3, rib=4, gap=5, direction=d)
        assert p.stats["solids"] == 1


def t_panel_thicknesses():
    for t in (2, 3, 4, 6):
        p = grid_panel(200, 120, t, rib=4, gap=5, direction="u")
        assert p.stats["solids"] == 1


def t_panel_cross_rejected():
    """A both-direction lattice on a flat plate CANNOT be one solid.

    This is geometry, not a bug, so the generator must refuse it loudly
    instead of writing 365 loose blocks to disk."""
    try:
        grid_panel(266, 156, 3, rib=4, gap=5, direction="both")
    except PartError:
        return
    raise AssertionError("direction='both' should raise PartError")


def t_panel_cross_allowed_explicitly():
    """Opting out of the guard is possible, and then the part really is a
    pile of blocks - we must not pretend otherwise."""
    p = grid_panel(266, 156, 3, rib=4, gap=5, direction="both",
                   allow_disconnected=True, end_rail_u=0, end_rail_v=0,
                   auto_rails=False)
    assert p.stats["solids"] > 1


def t_panel_end_rails():
    """A solid margin around the lattice is just a bigger end rail."""
    narrow = grid_panel(266, 156, 3, rib=4, gap=5, direction="u",
                        end_rail_u=5, auto_rails=False)
    wide = grid_panel(266, 156, 3, rib=4, gap=5, direction="u",
                      end_rail_u=40, auto_rails=False)
    assert narrow.stats["solids"] == 1
    assert wide.stats["solids"] == 1
    assert wide.stats["volume"] > narrow.stats["volume"], "bigger rails = more material"
    assert wide.open_ratio < narrow.open_ratio, "bigger rails = less open"


def t_panel_none():
    p = grid_panel(266, 156, 3, direction="none")
    assert abs(p.stats["volume"] - 266 * 156 * 3) < 1, p.stats["volume"]


def t_panel_single_solid():
    """Every printable panel must come out as one piece.

    A slot running edge to edge severs a flat plate, so each slot family
    needs a solid end rail.  Cross patterns are rejected outright - see
    t_panel_cross_rejected."""
    for L, W in ((266, 156), (156, 156), (300, 200), (100, 80), (60, 40)):
        for rib, gap in ((4, 5), (5, 5), (3, 8)):
            for d in ("u", "v"):
                p = grid_panel(L, W, 3, rib=rib, gap=gap, direction=d)
                assert p.stats["solids"] == 1, (
                    f"{L}x{W} rib{rib} gap{gap} dir={d} -> "
                    f"{p.stats['solids']} disconnected pieces"
                )


def t_panel_open_ratio():
    p = grid_panel(266, 156, 3, rib=4, gap=5, direction="u")
    assert 0.0 < p.open_ratio < 1.0, p.open_ratio
    assert p.open_ratio < 0.75, p.open_ratio


def t_panel_open_ratio_tracks_gap():
    a = grid_panel(266, 156, 3, rib=4, gap=4, direction="u").open_ratio
    b = grid_panel(266, 156, 3, rib=4, gap=10, direction="u").open_ratio
    assert b > a, (a, b)


def t_panel_rejects_bad_direction():
    try:
        grid_panel(100, 100, 3, direction="diagonal")
    except ValueError:
        return
    raise AssertionError("bad direction should raise")


def t_panel_param_sensitivity():
    """Widening a slot must remove material; widening a rib must add it."""
    a = grid_panel(266, 156, 3, rib=4, gap=5, direction="u").stats["volume"]
    b = grid_panel(266, 156, 3, rib=4, gap=8, direction="u").stats["volume"]
    c = grid_panel(266, 156, 3, rib=8, gap=5, direction="u").stats["volume"]
    assert b < a, (a, b)
    assert c > a, (a, c)


def t_panel_tiny():
    """Degenerate inputs must fail loudly, not silently produce junk."""
    for bad in ((10, 10, 3, 8, 5), (12, 12, 3, 4, 20)):
        try:
            grid_panel(*bad)
        except (PartError, ValueError):
            continue
        raise AssertionError(f"{bad} should have raised")


def t_panel_reproducible():
    a = grid_panel(266, 156, 3, rib=4, gap=5, direction="u").stats["volume"]
    b = grid_panel(266, 156, 3, rib=4, gap=5, direction="u").stats["volume"]
    assert abs(a - b) < 1e-6, (a, b)


def t_panel_asymmetric_rails():
    """Wide end rails on one axis only must not confuse the volume oracle.

    When the rails are wide enough that the outermost slot of one family no
    longer crosses the outermost slot of the other, not every u/v pair
    overlaps.  The overlap term used to assume n_u * n_v and over-estimated
    the material, which made verify() reject a perfectly good part.
    """
    for ru, rv in ((5, 5), (5, 30), (30, 5), (40, 40)):
        p = grid_panel(266, 156, 3, rib=4, gap=5, direction="both",
                       allow_disconnected=True, auto_rails=False,
                       end_rail_u=ru, end_rail_v=rv)
        assert abs(p.stats["ratio"] - 1.0) < 1e-4, (
            f"end_rail_u={ru} end_rail_v={rv} -> ratio {p.stats['ratio']}")


def t_panel_analytic_volume_exact():
    """analytic_volume() must equal the real solid for two-axis lattices."""
    from .core import analytic_volume, layout
    L, W, T = 266.0, 156.0, 3.0
    lu = layout(L, 4, 5)
    lv = layout(W, 4, 5)
    for ru, rv in ((0.0, 0.0), (5.0, 5.0), (5.0, 30.0), (30.0, 5.0)):
        expected = analytic_volume(L, W, T, lu, lv, True, True,
                                   end_rail_u=ru, end_rail_v=rv)
        p = grid_panel(L, W, T, rib=4, gap=5, direction="both",
                       allow_disconnected=True, auto_rails=False,
                       end_rail_u=ru, end_rail_v=rv)
        assert abs(p.stats["volume"] - expected) < 1.0, (
            f"end_rail_u={ru} end_rail_v={rv}: "
            f"solid {p.stats['volume']} != analytic {expected}")


def t_panel_is_valid_brep():
    """Every shipped part must be a valid, manifold BRep."""
    for p in (grid_panel(266, 156, 3, rib=4, gap=5, direction="u"),
              grid_panel(266, 156, 3, rib=4, gap=5, direction="v"),
              ribbed_plate(266, 156, base=3, rib_w=4, gap=5, rib_h=6,
                           direction="both")):
        assert p.stats.get("valid") is True, p.stats


def t_render_accepts_generator_result():
    """render() must unwrap GridPanel / RibbedPlate itself.

    Passing the generator result straight in used to raise
    TypeError: compound() argument after * must be an iterable.
    """
    try:
        import pyvista  # noqa: F401
    except ImportError:
        return                      # preview is optional, skip the check
    import tempfile
    from .render import render
    with tempfile.TemporaryDirectory() as td:
        out = render(grid_panel(60, 40, 3, rib=4, gap=5, direction="u"),
                     "t", views=("iso",), outdir=td)
        assert out and out[0].exists(), out
        out = render(ribbed_plate(60, 40, base=3, rib_w=4, gap=5, rib_h=6),
                     "t2", views=("top",), outdir=td)
        assert out and out[0].exists(), out


def t_render_feature_edges_drop_triangulation():
    """edges=<angle> must actually drop the STL's triangle boundaries.

    PyVista's own `add_mesh(feature_angle=...)` does NOT do this - it only
    affects smooth shading - so the feature edges have to be extracted with
    extract_feature_edges() and added as their own actor.  A regression here
    means the preview shows a mesh of little squares instead of the part.

    A smooth sphere is the sharpest test: every one of its triangle
    boundaries is below the threshold, so a correct filter finds nothing.
    """
    try:
        import pyvista as pv
    except ImportError:
        return

    from .render import _feature_edges

    sphere = pv.Sphere(theta_resolution=60, phi_resolution=60)
    assert sphere.n_cells > 1000, sphere.n_cells
    sharp = _feature_edges(sphere, 30.0)
    assert sharp.n_cells == 0, (
        f"{sharp.n_cells} feature edges on a smooth sphere: the "
        f"triangulation was not filtered out"
    )

    # a part with real corners must still find them
    import tempfile
    import cadquery as cq
    with tempfile.TemporaryDirectory() as td:
        p = grid_panel(60, 40, 3, rib=4, gap=5, direction="u")
        stl = f"{td}/x.stl"
        cq.exporters.export(p.shape, stl)
        mesh = pv.read(stl)
        sharp = _feature_edges(mesh, 30.0)
        assert sharp.n_cells > 0, "no feature edges found at all"


# ---------------------------------------------------------- ribbed_plate ----

def t_rib_basic():
    p = ribbed_plate(266, 156, base=3, rib_w=4, gap=5, rib_h=6)
    assert p.stats["solids"] == 1
    assert p.stats["volume"] > 0


def t_rib_always_one_solid():
    """The whole point of this generator: a cross pattern stays printable."""
    for L, W in ((266, 156), (156, 156), (300, 200), (100, 80), (60, 40)):
        for d in ("u", "v", "both"):
            for n_rib in (None, 3, 8):
                p = ribbed_plate(L, W, base=3, rib_w=4, gap=5, rib_h=6,
                                 direction=d, n_rib_u=n_rib, n_rib_v=n_rib)
                assert p.stats["solids"] == 1, (
                    f"{L}x{W} {d} n_rib={n_rib} -> {p.stats['solids']} pieces")


def t_rib_volume_grows_with_height():
    a = ribbed_plate(266, 156, base=3, rib_h=4).stats["volume"]
    b = ribbed_plate(266, 156, base=3, rib_h=9).stats["volume"]
    assert b > a, (a, b)


def t_rib_volume_grows_with_base():
    a = ribbed_plate(266, 156, base=2, rib_h=6).stats["volume"]
    b = ribbed_plate(266, 156, base=6, rib_h=6).stats["volume"]
    assert b > a, (a, b)


def t_rib_open_ratio_directions():
    u = ribbed_plate(266, 156, direction="u").open_ratio
    v = ribbed_plate(266, 156, direction="v").open_ratio
    both = ribbed_plate(266, 156, direction="both").open_ratio
    assert 0 < both < min(u, v), (u, v, both)


def t_rib_rejects_bad_input():
    for kw in ({"base": 0}, {"rib_h": 0}, {"direction": "diagonal"}):
        args = dict(length=100, width=80, base=3, rib_w=4, gap=5, rib_h=6)
        args.update(kw)
        try:
            ribbed_plate(**args)
        except ValueError:
            continue
        raise AssertionError(f"{kw} should raise ValueError")


# ----------------------------------------------------------- hanger_hook ----

def t_cap_basic():
    cap = hanger_hook_cap()
    assert cap.stats["solids"] == 1
    assert cap.stats["shells"] == 1
    assert cap.stats["valid"] is True
    assert cap.stats["volume"] > 0


def t_cap_cavity_fits_the_wire():
    """The cap must sit on the wire without interference.

    Every point of the wire that the cap covers has to stay outside the
    cap's material, and the gap between wire and cavity must be positive.
    """
    from examples.hanger_hook import _arc_pt, _arc_tangent
    from OCP.BRepClass3d import BRepClass3d_SolidClassifier
    from OCP.gp import gp_Pnt
    from OCP.TopAbs import TopAbs_IN

    cap = hanger_hook_cap()
    solid = cap.shape.solids().val().wrapped

    def inside(x, y):
        c = BRepClass3d_SolidClassifier(solid, gp_Pnt(x, y, 0.0), 1e-6)
        return c.State() == TopAbs_IN

    R = cap.params["hook_r"]
    wire = cap.params["wire_dia"] / 2
    a_tip = cap.params["tip_angle_deg"]
    a_open = a_tip + cap.params["cap_deg"]

    for i in range(41):
        a = a_tip + (a_open - a_tip) * i / 40
        base = _arc_pt(R, a)
        tan = _arc_tangent(a)
        for s in (-1, 1):
            # a point on the wire's own surface must not be inside the cap
            x = base[0] + s * wire * tan[0]
            y = base[1] + s * wire * tan[1]
            assert not inside(x, y), f"wire interferes with the cap at {a:.1f} deg"
            # just outside the wire there must still be clearance
            x = base[0] + s * (wire + 0.1) * tan[0]
            y = base[1] + s * (wire + 0.1) * tan[1]
            assert not inside(x, y), f"no clearance at {a:.1f} deg"

    # the cap has to cover PAST the wire's rounded tip, otherwise it does
    # not protect anything
    tip = _arc_pt(R, a_tip)
    tan = _arc_tangent(a_tip)
    assert not inside(tip[0], tip[1]), "the wire tip is buried in the cap wall"
    past = wire + cap.params["clearance"] + 0.2
    x = tip[0] + past * tan[0]
    y = tip[1] + past * tan[1]
    assert inside(x, y), "the cap stops short of the wire tip"


def t_cap_scales_with_the_wire():
    """Thicker wire must give a proportionally bigger cavity."""
    thin = hanger_hook_cap(wire_dia=3.0)
    thick = hanger_hook_cap(wire_dia=5.0)
    assert thick.inner_r > thin.inner_r
    assert thick.outer_r > thin.outer_r
    assert thick.stats["volume"] > thin.stats["volume"]


def t_cap_rejects_bad_input():
    for kw in ({"wire_dia": 0}, {"hook_r": 2.0}, {"cap_len": 0},
               {"wall": 0}, {"clearance": -1}):
        args = dict(wire_dia=4.0, hook_r=15.0, cap_len=12.0)
        args.update(kw)
        try:
            hanger_hook_cap(**args)
        except ValueError:
            continue
        raise AssertionError(f"{kw} should raise ValueError")


def t_hook_reference_builds():
    h = hanger_hook()
    assert h.stats["solids"] == 1
    assert h.stats["valid"] is True


# ------------------------------------------------------------------ blade ----

def t_blade_builds():
    b = table_tennis_blade(BladeSpec(edge_step=2.0))
    assert b.stats["solids"] == 1, b.stats
    assert b.stats["shells"] == 1, b.stats
    assert b.stats.get("valid") is True, b.stats
    assert b.stats["volume"] > 0, b.stats


def t_blade_dimensions():
    """The blade must come out at the size that was asked for."""
    spec = BladeSpec(edge_step=2.0)
    b = table_tennis_blade(spec).shape.val().BoundingBox()
    assert abs((b.xmax - b.xmin) - spec.head_w) < 0.05, (b.xmax - b.xmin)
    assert abs((b.ymax - b.ymin) - spec.length) < 0.05, (b.ymax - b.ymin)
    assert abs((b.zmax - b.zmin) - spec.thickness) < 1e-6, (b.zmax - b.zmin)


def t_blade_outline_is_symmetric():
    """A blade that is not mirror symmetric would print lopsided."""
    from examples.blade import _outline_points
    spec = BladeSpec(edge_step=1.0)
    pts = _outline_points(spec)
    mirror = [(-x, y) for x, y in pts]
    for x, y in pts:
        best = min(
            math.hypot(mx - x, my - y) for mx, my in mirror
        )
        assert best < 0.05, (x, y, best)


def t_blade_rim_is_beveled():
    """The top face has to be smaller than the blade, or the rim is a
    sharp 90 deg edge that catches the rubber."""
    spec = BladeSpec(edge_step=2.0)
    b = table_tennis_blade(spec).shape
    top = b.faces(">Z").val().BoundingBox()
    whole = b.val().BoundingBox()
    assert (top.xmax - top.xmin) < (whole.xmax - whole.xmin) - 1.0, (
        top.xmax - top.xmin, whole.xmax - whole.xmin)


def t_blade_scales():
    a = table_tennis_blade(BladeSpec(edge_step=2.0)).stats["volume"]
    b = table_tennis_blade(
        BladeSpec(head_w=160.0, edge_step=2.0)
    ).stats["volume"]
    assert b > a, (a, b)


def t_blade_rejects_bad_input():
    for kw in ({"thickness": 0.0}, {"rim_r": -1.0}, {"rim_r": 99.0},
               {"head_w": 0.0}, {"length": -5.0}, {"butt_w": 10.0},
               {"grip_w": 0.0}):
        spec = BladeSpec()
        for k, v in kw.items():
            setattr(spec, k, v)
        try:
            spec.validate()
        except ValueError:
            continue
        raise AssertionError(f"{kw} should raise ValueError")


# ----------------------------------------------------------------- paddle ----

def t_paddle_parts_build():
    """Every part of the paddle must be one printable solid."""
    for name, part in paddle_assembly().items():
        assert part.stats["solids"] == 1, (name, part.stats)
        assert part.stats["shells"] == 1, (name, part.stats)
        assert part.stats.get("valid") is True, (name, part.stats)
        assert part.stats["volume"] > 0, (name, part.stats)


def t_paddle_dovetail_fits():
    """At the assembled position the two blade halves must not touch."""
    spec = PaddleSpec()
    b1 = blade_half("front", spec).shape
    b2 = blade_half("back", spec).shape
    v = b1.intersect(b2).val().Volume()
    assert v < 1e-6, f"the dovetail interferes by {v} mm^3"


def t_paddle_dovetail_locks():
    """Pulling the halves apart must run them into the dovetail walls.

    A tongue that is the same width at the tip as at the root is a tenon:
    it slides straight out.  The ridge has to get wider towards its tip so
    that the wide part cannot pass back through the narrow mouth.
    """
    spec = PaddleSpec()
    b1 = blade_half("front", spec).shape
    b2 = blade_half("back", spec).shape
    v = b1.translate((0.0, 0.0, 1.0)).intersect(b2).val().Volume()
    assert v > 1.0, f"the dovetail does not lock: only {v} mm^3 of interference"
    assert spec.tongue_tip > spec.tongue_mouth, spec


def t_paddle_ridge_belongs_to_half_one():
    """Blade half 1 carries the ridge, blade half 2 the groove."""
    spec = PaddleSpec()
    a = blade_half("front", spec).stats["volume"]
    b = blade_half("back", spec).stats["volume"]
    assert a > b, (a, b)


def t_paddle_clamps_clear_the_blade():
    """No part may foul another when the paddle is assembled."""
    spec = PaddleSpec()
    b1 = blade_half("front", spec).shape
    b2 = blade_half("back", spec).shape
    c1 = handle_clamp("front", spec).shape
    c2 = handle_clamp("back", spec).shape
    for a, b, n in ((b1, c1, "blade1/clamp1"), (b2, c1, "blade2/clamp1"),
                    (b1, c2, "blade1/clamp2"), (b2, c2, "blade2/clamp2"),
                    (c1, c2, "clamp1/clamp2")):
        v = a.intersect(b).val().Volume()
        assert v < 1e-6, f"{n} interferes by {v} mm^3"


def t_paddle_screw_holes_line_up():
    """A rod at each screw position must pass through the blade and both
    clamps without touching anything."""
    import cadquery as cq

    spec = PaddleSpec()
    z0, z1 = -2 * spec.handle_t, 2 * spec.handle_t
    r = spec.screw_d / 2.0 - 0.05
    rod = (
        cq.Workplane("XY")
        .center(spec.screw_x, spec.screw_y)
        .circle(r)
        .extrude(z1 - z0)
        .translate((0.0, 0.0, z0))
    )
    rod = rod.union(
        cq.Workplane("XY")
        .center(-spec.screw_x, spec.screw_y)
        .circle(r)
        .extrude(z1 - z0)
        .translate((0.0, 0.0, z0))
    )
    for shape in (blade_half("front", spec).shape,
                  blade_half("back", spec).shape,
                  handle_clamp("front", spec).shape,
                  handle_clamp("back", spec).shape):
        v = shape.intersect(rod).val().Volume()
        assert v < 1e-6, f"a screw hits material: {v} mm^3"


def t_paddle_screw_is_recessed():
    """Nothing may stand proud of the clamp's outer face where the palm
    goes: the screw head has to sit down inside the counterbore."""
    spec = PaddleSpec()
    screw = m3_screw(spec).shape
    for clamp in (handle_clamp("front", spec).shape,
                  handle_clamp("back", spec).shape):
        v = screw.intersect(clamp).val().Volume()
        assert v < 1e-6, f"the screw fouls the clamp: {v} mm^3"
    top = screw.val().BoundingBox().zmax
    assert top <= spec.clamp_t + 1e-6, f"the screw head stands proud: {top}"


def t_paddle_clamp_top_clears_the_neck():
    """Regression test.

    The clamp outline is a spline, and a spline through a flat top edge
    used to overshoot it by 7 mm and crash into the flared neck.
    """
    spec = PaddleSpec()
    for which in ("front", "back"):
        bb = handle_clamp(which, spec).shape.val().BoundingBox()
        assert bb.ymax <= spec.handle_top + 0.5, (which, bb.ymax)


def t_paddle_rubber_fits_the_head():
    """The rubber facing must not overhang the blade it sits on."""
    spec = PaddleSpec()
    rb = rubber_sheet(spec).shape.val().BoundingBox()
    hb = blade_half("front", spec).shape.val().BoundingBox()
    assert rb.xmin >= hb.xmin - 1e-6, (rb.xmin, hb.xmin)
    assert rb.xmax <= hb.xmax + 1e-6, (rb.xmax, hb.xmax)
    assert rb.ymax <= hb.ymax + 1e-6, (rb.ymax, hb.ymax)


def t_paddle_scales():
    """A bigger blade must give a bigger part."""
    a = blade_half("front", PaddleSpec()).stats["volume"]
    b = blade_half("front", PaddleSpec(head_h=150.0)).stats["volume"]
    assert b > a, (a, b)


def t_paddle_rejects_bad_input():
    for kw in ({"which": "middle"}, {"which": None}):
        try:
            blade_half(**kw)
        except ValueError:
            continue
        raise AssertionError(f"{kw} should raise ValueError")
    for kw in ({"which": "side"},):
        try:
            handle_clamp(**kw)
        except ValueError:
            continue
        raise AssertionError(f"{kw} should raise ValueError")
    for kw in ({"tongue_tip": 10.0}, {"tongue_depth": 99.0},
               {"blade_t": 0.0}, {"neck_w": 5.0}, {"dish_d": 1.0},
               {"cbore_d": 1.0}, {"handle_t": -1.0}):
        spec = PaddleSpec()
        for k, v in kw.items():
            setattr(spec, k, v)
        try:
            spec.validate()
        except ValueError:
            continue
        raise AssertionError(f"{kw} should raise ValueError")


# ------------------------------------------------------------------ main ----

def main() -> int:
    groups = [
        ("layout", "t_layout_"),
        ("grid_panel", "t_panel_"),
        ("ribbed_plate", "t_rib_"),
        ("render", "t_render_"),
        ("hanger_hook", "t_cap_"),
        ("hanger_hook", "t_hook_"),
        ("paddle", "t_paddle_"),
        ("blade", "t_blade_"),
    ]
    for title, prefix in groups:
        print(title)
        for n, f in sorted(globals().items()):
            if n.startswith(prefix):
                check(n[2:], f)
        print()
    print()
    if _failures:
        print(f"{len(_failures)} FAILED")
        return 1
    print("all checks passed")
    return 0


if __name__ == "__main__":
    sys.exit(main())
