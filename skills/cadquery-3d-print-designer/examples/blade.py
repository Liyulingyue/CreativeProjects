"""blade.py - a one-piece printed table tennis blade.

The quick-release paddle (see paddle.py) is a mechanism exercise.  This is
the thing people actually want: a single printed blade you can put rubber
on.

The shape follows a real wooden blade:

    * an egg-shaped head, widest a little below the middle
    * a graceful concave "throat" where the head runs into the grip
    * a straight, slightly waisted grip
    * a flared skirt at the butt, so the handle does not slip out of your
      hand when you swing
    * a rounded bull-nose rim, not a sharp 90 deg edge - that is what a
      wooden blade has, and it is also kinder to the rubber and to the
      opponent's table edge

The outline is sampled from a width profile with a shape-preserving cubic
(PCHIP) interpolation, so the curve never overshoots: no bulges in the
grip, no pointy top, and the widest point is exactly where it was asked
for.

Printing
--------
Lay the blade face down on the bed.  The rim is a bull-nose, so the whole
thing is one layer thick everywhere and needs no support at all.  Print it
in PETG or ABS if you have them: a 6 mm plate that is 250 mm long will warp
in PLA unless it is kept in a warm box.
"""
from __future__ import annotations

import math
from dataclasses import dataclass, field

import cadquery as cq

from pipeline.core import PartError, verify


# ------------------------------------------------------------------ spec ----

@dataclass
class BladeSpec:
    """Every dimension of the blade, in millimetres."""

    length: float = 255.0        # butt to the top of the head
    head_w: float = 150.0        # widest part of the head
    thickness: float = 6.0
    rim_r: float = 1.2           # bevel width on the rim
    grip_w: float = 34.0         # width of the grip
    butt_w: float = 41.0         # width across the skirt at the butt
    edge_step: float = 1.5       # how finely the outline is sampled

    @property
    def head_half(self) -> float:
        return self.head_w / 2.0

    @property
    def grip_half(self) -> float:
        return self.grip_w / 2.0

    @property
    def butt_half(self) -> float:
        return self.butt_w / 2.0

    def validate(self) -> "BladeSpec":
        if self.length <= 0:
            raise ValueError(f"length must be > 0, got {self.length}")
        if self.head_w <= 0:
            raise ValueError(f"head_w must be > 0, got {self.head_w}")
        if self.thickness <= 0:
            raise ValueError(f"thickness must be > 0, got {self.thickness}")
        if self.rim_r <= 0:
            raise ValueError(f"rim_r must be > 0, got {self.rim_r}")
        if self.rim_r >= self.thickness / 2.0:
            raise ValueError(
                f"rim_r {self.rim_r} is too big for a {self.thickness} mm "
                f"blade: the bull-nose would meet in the middle"
            )
        if self.grip_w <= 0 or self.butt_w <= 0:
            raise ValueError("the grip and the butt must have a positive width")
        if self.butt_w <= self.grip_w:
            raise ValueError(
                f"the butt ({self.butt_w}) must be wider than the grip "
                f"({self.grip_w}) or the skirt does not flare"
            )
        if self.head_w <= self.butt_w:
            raise ValueError("the head must be wider than the butt")
        return self


# ----------------------------------------------------------------- part ----

@dataclass
class Blade:
    shape: cq.Workplane
    stats: dict
    params: dict = field(default_factory=dict)

    def save(self, path) -> None:
        cq.exporters.export(self.shape, str(path))

    def __str__(self) -> str:
        p = self.params
        return (
            f"{p['label']:<20} volume {self.stats['volume']:>10,.1f} mm^3  "
            f"faces {self.stats['faces']:>3}  "
            f"mass {p.get('mass_g', 0.0):>6.1f} g"
        )


# --------------------------------------------------------------- profile ----

# (station as a fraction of the length, half width as a fraction of the
# head's half width).  The head, from the very top down to the throat.
# The widest point is deliberately BELOW the middle: a real blade is egg
# shaped, not a circle.
_HEAD = [
    (0.000, 0.000),
    (0.003, 0.140),
    (0.010, 0.275),
    (0.020, 0.410),
    (0.034, 0.540),
    (0.052, 0.655),
    (0.075, 0.755),
    (0.104, 0.838),
    (0.138, 0.900),
    (0.178, 0.947),
    (0.222, 0.980),
    (0.270, 0.996),
    (0.318, 1.000),
    (0.366, 0.994),
    (0.408, 0.972),
    (0.446, 0.942),
    (0.478, 0.902),
    (0.506, 0.848),
    (0.528, 0.780),
    (0.545, 0.695),
    (0.556, 0.595),
    (0.566, 0.480),
]

# (station, half width as a fraction of the grip's half width).  The
# throat, the grip, and the flared skirt at the butt.
_TAIL = [
    (0.604, 0.700),
    (0.648, 0.830),
    (0.686, 0.930),
    (0.712, 0.950),
    (0.745, 0.990),
    (0.790, 1.010),
    (0.850, 1.020),
    (0.900, 1.030),
    (0.930, 1.055),
    (0.950, 1.130),
    (0.960, 1.205),
]


def _pchip_slopes(xs: list, ys: list) -> list:
    """Fritsch-Carlson slopes: a cubic that never overshoots its data."""
    n = len(xs)
    h = [xs[i + 1] - xs[i] for i in range(n - 1)]
    d = [(ys[i + 1] - ys[i]) / h[i] for i in range(n - 1)]
    m = [0.0] * n
    m[0] = d[0]
    m[-1] = d[-1]
    for i in range(1, n - 1):
        if d[i - 1] * d[i] <= 0.0:
            m[i] = 0.0
        else:
            w1 = 2.0 * h[i] + h[i - 1]
            w2 = h[i] + 2.0 * h[i - 1]
            m[i] = (w1 + w2) / (w1 / d[i - 1] + w2 / d[i])
    return m


def _pchip(xs: list, ys: list, x: float) -> float:
    """Evaluate the shape-preserving cubic at `x`."""
    n = len(xs)
    if x <= xs[0]:
        return ys[0]
    if x >= xs[-1]:
        return ys[-1]
    i = 0
    while xs[i + 1] < x:
        i += 1
    h = xs[i + 1] - xs[i]
    t = (x - xs[i]) / h
    t2 = t * t
    t3 = t2 * t
    m = _pchip_slopes(xs, ys)
    return (
        (2 * t3 - 3 * t2 + 1) * ys[i]
        + (t3 - 2 * t2 + t) * h * m[i]
        + (-2 * t3 + 3 * t2) * ys[i + 1]
        + (t3 - t2) * h * m[i + 1]
    )


def _signed_area(points: list) -> float:
    a = 0.0
    for (x0, y0), (x1, y1) in zip(points, points[1:] + points[:1]):
        a += x0 * y1 - x1 * y0
    return a / 2.0


def _offset_inward(points: list, r: float) -> list:
    """Offset a closed polyline inward by `r`, with mitred corners.

    Every edge is pushed along its inward normal by r and the two offset
    edges around a vertex are intersected.  For a finely sampled curve the
    miter is a very good stand-in for the true offset, and unlike OCCT's
    fillet builder it has no opinion about how many edges a face may have.
    """
    n = len(points)
    ccw = _signed_area(points) > 0.0
    sign = 1.0 if ccw else -1.0
    out = []
    for i in range(n):
        p0 = points[(i - 1) % n]
        p1 = points[i]
        p2 = points[(i + 1) % n]
        d1 = (p1[0] - p0[0], p1[1] - p0[1])
        l1 = math.hypot(*d1)
        d2 = (p2[0] - p1[0], p2[1] - p1[1])
        l2 = math.hypot(*d2)
        if l1 < 1e-12 or l2 < 1e-12:
            out.append(p1)
            continue
        n1 = (-sign * d1[1] / l1, sign * d1[0] / l1)
        n2 = (-sign * d2[1] / l2, sign * d2[0] / l2)
        # the offset of edge p0->p1 and of edge p1->p2
        a = (p0[0] + n1[0] * r, p0[1] + n1[1] * r)
        b = (p1[0] + n2[0] * r, p1[1] + n2[1] * r)
        # intersect the line a + t*d1 with the line b + u*d2
        den = d1[0] * d2[1] - d1[1] * d2[0]
        if abs(den) < 1e-12:
            out.append(b)
            continue
        t = ((b[0] - a[0]) * d2[1] - (b[1] - a[1]) * d2[0]) / den
        out.append((a[0] + d1[0] * t, a[1] + d1[1] * t))
    return out


def _curvature_radius(a, b, c) -> float:
    """Radius of the circle through three points (huge for a straight run)."""
    ax, ay = b[0] - a[0], b[1] - a[1]
    bx, by = c[0] - b[0], c[1] - b[1]
    cross = ax * by - ay * bx
    if abs(cross) < 1e-12:
        return 1e9
    la = math.hypot(ax, ay)
    lb = math.hypot(bx, by)
    lc = math.hypot(c[0] - a[0], c[1] - a[1])
    return la * lb * lc / (2.0 * abs(cross))


def _resample(points: list, min_step: float, max_step: float) -> list:
    """Resample a polyline, finely where it curves and coarsely where it
    does not.

    Two things depend on this.  A spline through unevenly spaced points
    wobbles, and it overshoots the ends; and OCCT's fillet builder gives
    up on a face whose edge loop has more than about 180 edges.  Sampling
    by curvature gives a smooth, evenly weighted curve AND keeps the
    point count inside what the fillet can handle.
    """
    n = len(points)
    if n < 3:
        return list(points)
    if min_step >= max_step:
        steps = [min_step] * n
    else:
        steps = []
        for i in range(n):
            i0 = max(0, i - 1)
            i2 = min(n - 1, i + 1)
            r = _curvature_radius(points[i0], points[i], points[i2])
            steps.append(min(max(0.55 * math.sqrt(r), min_step), max_step))

    out = [points[0]]
    acc = 0.0
    for i in range(n - 1):
        p, q = points[i], points[i + 1]
        seg = math.hypot(q[0] - p[0], q[1] - p[1])
        if seg <= 1e-9:
            continue
        t = 0.0
        while True:
            need = steps[i] - acc
            if need > seg - t:
                acc += seg - t
                break
            t += need
            acc = 0.0
            out.append((p[0] + (q[0] - p[0]) * t / seg,
                        p[1] + (q[1] - p[1]) * t / seg))
    out.append(points[-1])
    return out


def _half_profile(spec: BladeSpec, per_seg: int = 6) -> list:
    """The right-hand edge of the blade, from the top of the head down to
    the butt, as a list of (x, y)."""
    L = spec.length
    stations = [(f, w * spec.head_half) for f, w in _HEAD]
    stations += [(f, w * spec.grip_half) for f, w in _TAIL]
    # the skirt flares to its widest and then runs dead straight for the
    # last few millimetres.  The straight run matters: the side has to
    # meet the corner round tangentially, or the outline has a 0.3 mm
    # radius kink there and OCCT refuses to fillet the rim.
    stations[-2] = (0.965, spec.butt_half)
    stations[-1] = (0.973, spec.butt_half)

    fs = [s[0] for s in stations]
    ws = [s[1] for s in stations]
    if any(fs[i] >= fs[i + 1] for i in range(len(fs) - 1)):
        raise AssertionError("the blade width stations must increase")

    out = []
    for i in range(len(fs) - 1):
        for k in range(per_seg):
            f = fs[i] + (fs[i + 1] - fs[i]) * k / per_seg
            out.append((_pchip(fs, ws, f), f * L))
    out.append((ws[-1], fs[-1] * L))
    return out


def _outline_points(spec: BladeSpec) -> list:
    """The closed outline of the blade, as a list of (x, y).

    The right half is walked from the top of the head, down the side, round
    the corner of the skirt and across the flat butt; the left half is its
    mirror.  The whole lot is then resampled at a uniform arc length, so
    the spline that is fitted to it later has an even parameterisation and
    does not wobble.
    """
    right = _half_profile(spec)

    # the skirt ends at the widest point of the flare; from there the
    # outline turns through a quarter round into the flat butt
    bx, by = right[-1]
    r = spec.length - by
    arc = []
    for i in range(1, 7):
        th = (math.pi / 2.0) * i / 6.0
        arc.append((bx - r + r * math.cos(th), by + r * math.sin(th)))
    half = _resample(right + arc + [(0.0, spec.length)], spec.edge_step, spec.edge_step)

    pts = list(half)
    for x, y in reversed(half[1:-1]):
        pts.append((-x, y))
    return pts


# ---------------------------------------------------------------- build ----

def table_tennis_blade(spec: BladeSpec | None = None) -> Blade:
    """Build the one-piece blade."""
    spec = (spec or BladeSpec()).validate()

    pts = _outline_points(spec)
    shape = (
        cq.Workplane("XY")
        .polyline(pts)
        .close()
        .extrude(spec.thickness)
    )

    # Break the sharp rim: cut a bevel off the top and the bottom, the
    # way a real blade is eased off so the rubber has nothing to catch on.
    # A boolean is used rather than a 3D fillet because OCCT's fillet
    # builder gives up on a face whose edge loop has more than ~180 edges,
    # and a smooth outline needs a lot of them.
    inner = _offset_inward(pts, spec.rim_r)
    for z0, z1 in ((spec.thickness - spec.rim_r, spec.thickness),
                   (0.0, spec.rim_r)):
        ring = (
            cq.Workplane("XY")
            .polyline(pts)
            .close()
            .extrude(z1 - z0)
            .translate((0.0, 0.0, z0))
        )
        hole = (
            cq.Workplane("XY")
            .polyline(inner)
            .close()
            .extrude(z1 - z0)
            .translate((0.0, 0.0, z0))
        )
        shape = shape.cut(ring.cut(hole))

    stats = verify(shape, label="table_tennis_blade")
    params = dict(
        label="table_tennis_blade",
        length=spec.length,
        head_w=spec.head_w,
        thickness=spec.thickness,
        grip_w=spec.grip_w,
        butt_w=spec.butt_w,
        rim_r=spec.rim_r,
    )
    params["mass_g"] = stats["volume"] / 1000.0 * 1.24
    return Blade(shape, stats, params)
