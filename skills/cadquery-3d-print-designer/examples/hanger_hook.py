"""hanger_hook_cap - a protective sleeve for the tip of a clothes hanger hook.

A hanger hook is a bent steel wire.  Its last 10-15 mm curl inwards and the
pointed end of that curl is what scratches the closet rail, snags on other
hangers and catches the shoulder of a coat.  This generator makes a small
cap that slides over that tip and turns it into a smooth plastic dome.

The cap is a hollow sleeve that follows the hook's own curve:

    wire centreline  --->  cavity (r_in)  --->  cap wall  -->  outer (r_out)

    r_in  = wire_dia/2 + clearance
    r_out = r_in + wall

so the cap fits any wire diameter and any hook curvature without the
geometry having to be re-derived: change `wire_dia`, `hook_r` and the
numbers follow.

Everything is parametric:

    from examples.hanger_hook import hanger_hook_cap
    cap = hanger_hook_cap(wire_dia=4.0, hook_r=15.0, cap_len=12.0)
    cap.save("hook_cap.stl")

Printing
--------
Lay the cap mouth-down and dome-up.  The bore is then a straight vertical
hole, so it needs no support, and the dome bridges over itself.  0.2 mm
layers, no supports, no brim beyond the usual one.

`hanger_hook()` builds a reference hook (stem + bend + curled tip) so you can
see the cap in context and check the fit.
"""
from __future__ import annotations

import math
from dataclasses import dataclass, field

import cadquery as cq

from pipeline.core import PartError, verify


# --------------------------------------------------------------- geometry ----

def _arc_pt(r: float, deg: float) -> tuple[float, float]:
    """Point on a circle of radius `r` at `deg` (CCW from +X)."""
    a = math.radians(deg)
    return (r * math.cos(a), r * math.sin(a))


def _arc_tangent(deg: float) -> tuple[float, float]:
    """Unit tangent of a circle at `deg`, travelling with DECREASING angle.

    Decreasing angle is the direction the hook is drawn in: up the stem,
    over the top, down the far side, into the tip curl.
    """
    a = math.radians(deg)
    return (math.sin(a), -math.cos(a))


def _profile_plane(point, tangent) -> cq.Plane:
    """A plane through `point` whose normal is `tangent`.

    The profile circle has to be perpendicular to the path, otherwise the
    sweep twists and the solid comes out with the wrong volume.
    """
    n = math.hypot(*tangent)
    tx, ty = tangent[0] / n, tangent[1] / n
    return cq.Plane(
        origin=cq.Vector(point[0], point[1], 0.0),
        xDir=cq.Vector(ty, -tx, 0.0),
        normal=cq.Vector(tx, ty, 0.0),
    )


def _sphere(r: float, at) -> cq.Workplane:
    return cq.Workplane("XY").sphere(r).translate((at[0], at[1], 0.0))


# ------------------------------------------------------------------- cap ----

@dataclass
class HookCap:
    shape: cq.Workplane
    stats: dict
    params: dict = field(default_factory=dict)

    def save(self, path) -> None:
        cq.exporters.export(self.shape, str(path))

    @property
    def inner_r(self) -> float:
        return self.params["r_in"]

    @property
    def outer_r(self) -> float:
        return self.params["r_out"]

    def __str__(self) -> str:
        p = self.params
        return (
            f"{p['label']}  wire {p['wire_dia']}  hook R {p['hook_r']}  "
            f"cap {p['cap_len']} long\n"
            f"  cavity r {p['r_in']:.2f}  outer r {p['r_out']:.2f}  "
            f"wall {p['wall']}\n"
            f"  covers {p['cap_deg']:.1f} deg of the bend, "
            f"tip at {p['tip_angle_deg']} deg\n"
            f"  mass {p['mass_g']:.2f} g"
        )


def hanger_hook_cap(
    wire_dia: float = 4.0,
    hook_r: float = 15.0,
    cap_len: float = 12.0,
    *,
    clearance: float = 0.3,
    wall: float = 1.8,
    lead_in: float = 0.6,
    tip_angle_deg: float = -70.0,
    label: str = "hanger_hook_cap",
) -> HookCap:
    """Build the sleeve that goes over the tip of a hanger hook.

    wire_dia    diameter of the hanger's wire
    hook_r      centreline radius of the hook's bend at the tip
    cap_len     how much of the bend the cap covers, measured along the wire
    clearance   radial gap between the wire and the cavity (sliding fit)
    wall        wall thickness of the cap
    lead_in     extra opening at the mouth, so it eases onto the wire
    tip_angle_deg  where the tip sits on the bend; used to place the cap
                   relative to the reference hook
    """
    if wire_dia <= 0:
        raise ValueError(f"wire_dia must be > 0, got {wire_dia}")
    if hook_r <= wire_dia:
        raise ValueError(
            f"hook_r {hook_r} must be larger than the wire diameter "
            f"{wire_dia}, otherwise the hook cannot be bent"
        )
    if cap_len <= 0:
        raise ValueError(f"cap_len must be > 0, got {cap_len}")
    if wall <= 0:
        raise ValueError(f"wall must be > 0, got {wall}")
    if clearance < 0:
        raise ValueError(f"clearance must be >= 0, got {clearance}")

    r_in = wire_dia / 2.0 + clearance
    r_out = r_in + wall
    if r_out >= hook_r:
        raise ValueError(
            f"the cap is too fat for this bend: outer radius {r_out} >= "
            f"hook radius {hook_r}, it would swallow the hook's inside"
        )

    cap_deg = math.degrees(cap_len / hook_r)
    a_tip = tip_angle_deg
    a_open = a_tip + cap_deg

    p_open = _arc_pt(hook_r, a_open)
    p_mid = _arc_pt(hook_r, (a_tip + a_open) / 2.0)
    p_tip = _arc_pt(hook_r, a_tip)

    path = cq.Workplane("XY").moveTo(*p_open).threePointArc(p_mid, p_tip)
    plane = _profile_plane(p_open, _arc_tangent(a_open))

    # cavity: the volume the wire occupies, plus the rounded end of the wire
    cavity = cq.Workplane(plane).circle(r_in).sweep(path, isFrenet=True)
    cavity = cavity.union(_sphere(r_in, p_tip))

    # outer: the same sweep, one wall thickness further out, domed at the tip
    outer = cq.Workplane(plane).circle(r_out).sweep(path, isFrenet=True)
    outer = outer.union(_sphere(r_out, p_tip))

    shape = outer.cut(cavity)

    # lead-in: widen the mouth a touch so the cap finds the wire
    if lead_in > 0:
        inward = _arc_tangent(a_open)
        cut_plane = _profile_plane(p_open, (-inward[0], -inward[1]))
        lead = (
            cq.Workplane(cut_plane)
            .circle(r_in + lead_in)
            .workplane(offset=lead_in * 2)
            .circle(r_in)
            .loft()
        )
        shape = shape.cut(lead)

    stats = verify(shape, label=label)

    # 1.24 g/cm^3 is a decent average for PLA/PETG
    mass_g = stats["volume"] / 1000.0 * 1.24

    params = dict(
        label=label,
        wire_dia=wire_dia,
        hook_r=hook_r,
        cap_len=cap_len,
        cap_deg=cap_deg,
        clearance=clearance,
        wall=wall,
        lead_in=lead_in,
        tip_angle_deg=tip_angle_deg,
        r_in=r_in,
        r_out=r_out,
        mass_g=mass_g,
    )
    return HookCap(shape, stats, params)


# ---------------------------------------------------------- reference hook ---

@dataclass
class Hook:
    shape: cq.Workplane
    stats: dict
    params: dict = field(default_factory=dict)

    def save(self, path) -> None:
        cq.exporters.export(self.shape, str(path))

    def __str__(self) -> str:
        p = self.params
        return (
            f"reference hook  wire {p['wire_dia']}  R {p['hook_r']}  "
            f"stem {p['stem_h']}  tip at {p['tip_angle_deg']} deg"
        )


def hanger_hook(
    wire_dia: float = 4.0,
    hook_r: float = 15.0,
    stem_h: float = 40.0,
    *,
    stem_angle_deg: float = 180.0,
    tip_angle_deg: float = -70.0,
    label: str = "reference_hook",
) -> Hook:
    """A plain hanger hook: straight stem, bend, inward-curling tip.

    This is only there so the cap can be seen in context and its fit
    checked.  It is not meant to be printed - a real hook is bent wire.
    """
    if wire_dia <= 0:
        raise ValueError(f"wire_dia must be > 0, got {wire_dia}")
    if hook_r <= wire_dia:
        raise ValueError(f"hook_r {hook_r} must exceed wire_dia {wire_dia}")

    r = wire_dia / 2.0
    p_stem_top = _arc_pt(hook_r, stem_angle_deg)
    p_stem_bot = (p_stem_top[0], p_stem_top[1] - stem_h)
    p_mid = _arc_pt(hook_r, (stem_angle_deg + tip_angle_deg) / 2.0)
    p_tip = _arc_pt(hook_r, tip_angle_deg)

    path = (
        cq.Workplane("XY")
        .moveTo(*p_stem_bot)
        .lineTo(*p_stem_top)
        .threePointArc(p_mid, p_tip)
    )
    plane = _profile_plane(p_stem_bot, (0.0, 1.0))
    shape = cq.Workplane(plane).circle(r).sweep(path, isFrenet=True)
    shape = shape.union(_sphere(r, p_tip))

    stats = verify(shape, label=label)
    params = dict(
        label=label,
        wire_dia=wire_dia,
        hook_r=hook_r,
        stem_h=stem_h,
        stem_angle_deg=stem_angle_deg,
        tip_angle_deg=tip_angle_deg,
    )
    return Hook(shape, stats, params)
