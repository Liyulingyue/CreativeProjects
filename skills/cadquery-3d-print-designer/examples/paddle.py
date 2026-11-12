"""paddle.py - a quick-release table tennis paddle, built from five part kinds.

The paddle is not one printed body.  It is an assembly:

    1. rubber sheet   - the sponge/rubber facing, one per side
    2. blade half 1   - the front half of the blade, carries the dovetail ridge
    3. blade half 2   - the back half of the blade, carries the dovetail groove
    4. handle clamp 1 - the front half of the handle
    5. handle clamp 2 - the back half of the handle

How it holds together
---------------------
The blade is split along its mid-plane (z = 0) all the way round, so the two
blade halves are two flat plates of half the thickness.  All the mechanical
work of that joint is done by ONE dovetail in the neck (the tang):

    blade half 1 (front)   carries a trapezoidal RIDGE hanging down
    blade half 2 (back)    carries the matching trapezoidal GROOVE

The ridge is WIDE at its tip and NARROW at its root.  That is the dovetail
trick: once the ridge is in the groove the two halves cannot be pulled apart
any more, so the blade body needs no screws of its own and the joint has no
visible seam on the hitting surface.

The handle clamps do a different job.  They close around the neck and are
pulled tight by two M3 screws.  The screws are recessed twice over:

    * a counterbore in the OUTER face of each clamp swallows the screw head
      and the nut, so nothing stands proud where the hand goes
    * a wider, shallow dish around each counterbore gives the head a pocket
      to sit in - that is the "凹陷" that stops the screw digging into the
      palm

Because the blade is held by the dovetail and the handle only by two
screws, either group can be swapped without touching the other: that is the
quick release.

Printing
--------
blade half 1  - lay the flat face down.  The ridge points up and is wider at
                the top: a 34 deg overhang, no support needed.
blade half 2  - lay the flat face down.  The groove is an open slot, there
                is nothing to bridge.
handle clamps  - lay the OUTER face down.  The channel that receives the
                neck opens sideways and the counterbores are on the bed.
rubber         - cut from a commercial sheet, 1.9 mm sponge.
"""
from __future__ import annotations

from dataclasses import dataclass, field

import cadquery as cq

from pipeline.core import PartError, verify


# ------------------------------------------------------------------ spec ----

@dataclass
class PaddleSpec:
    """Every dimension of the paddle, in millimetres.

    The defaults are a legal-ish ITTF paddle: 150 mm wide blade, 6 mm
    thick, 28 mm handle, M3 screws.
    """

    # --- blade head ------------------------------------------------------
    head_w: float = 150.0
    head_h: float = 135.0

    # --- neck (the tang that reaches down into the handle) ----------------
    neck_len: float = 38.0
    neck_w: float = 36.0
    neck_top_w: float = 66.0
    neck_r: float = 8.0

    # --- thicknesses -----------------------------------------------------
    blade_t: float = 6.0

    # --- the dovetail ----------------------------------------------------
    tongue_y0: float = -16.0
    tongue_y1: float = -4.0
    tongue_mouth: float = 22.0        # width at the root (z = 0)
    tongue_tip: float = 26.0          # width at the tip  (z = -tongue_depth)
    tongue_depth: float = 3.0
    fit: float = 0.25                 # print clearance on the dovetail

    # --- handle clamps ---------------------------------------------------
    handle_top: float = -20.0         # y of the top of the clamp
    handle_len: float = 84.0
    handle_t: float = 28.0            # total, both clamps together

    # --- screws ----------------------------------------------------------
    screw_x: float = 8.0
    screw_y: float = -28.0
    screw_d: float = 3.2
    cbore_d: float = 7.0
    cbore_depth: float = 3.5
    dish_d: float = 13.0
    dish_depth: float = 1.5

    # --- rubber ----------------------------------------------------------
    rubber_t: float = 1.9
    rubber_inset: float = 1.0

    def validate(self) -> "PaddleSpec":
        """Reject a spec that cannot make a working paddle.

        Called by every generator, so a nonsense parameter fails with a
        message that says why instead of producing a part that cannot be
        assembled.
        """
        if self.head_w <= 0 or self.head_h <= 0:
            raise ValueError("the blade head must have a positive size")
        if self.blade_t <= 0:
            raise ValueError(f"blade_t must be > 0, got {self.blade_t}")
        if self.neck_w <= 0 or self.neck_len <= 0:
            raise ValueError("the neck must have a positive size")
        if self.tongue_tip <= self.tongue_mouth:
            raise ValueError(
                f"the dovetail must get WIDER towards its tip or it is a "
                f"tenon, not a dovetail: tip {self.tongue_tip} <= "
                f"mouth {self.tongue_mouth}"
            )
        if self.tongue_tip >= self.neck_w:
            raise ValueError(
                f"the dovetail ({self.tongue_tip} wide) does not fit in the "
                f"neck ({self.neck_w} wide)"
            )
        if self.tongue_depth > self.half_t:
            raise ValueError(
                f"the dovetail is {self.tongue_depth} deep but the blade is "
                f"only {self.blade_t} thick, so the ridge would poke out of "
                f"the back half"
            )
        if self.handle_t <= 0 or self.handle_len <= 0:
            raise ValueError("the handle must have a positive size")
        if self.screw_d <= 0:
            raise ValueError(f"screw_d must be > 0, got {self.screw_d}")
        if self.cbore_d <= self.screw_d:
            raise ValueError(
                f"the counterbore ({self.cbore_d}) must be wider than the "
                f"screw ({self.screw_d})"
            )
        if self.dish_d <= self.cbore_d:
            raise ValueError(
                f"the dish ({self.dish_d}) must be wider than the counterbore "
                f"({self.cbore_d})"
            )
        if self.cbore_depth <= 0 or self.dish_depth <= 0:
            raise ValueError("the recess depths must be positive")
        if self.rubber_t <= 0:
            raise ValueError(f"rubber_t must be > 0, got {self.rubber_t}")
        return self

    @property
    def half_t(self) -> float:
        return self.blade_t / 2.0

    @property
    def clamp_t(self) -> float:
        return self.handle_t / 2.0


# ----------------------------------------------------------------- part ----

@dataclass
class PaddlePart:
    shape: cq.Workplane
    stats: dict
    params: dict = field(default_factory=dict)

    def save(self, path) -> None:
        cq.exporters.export(self.shape, str(path))

    def __str__(self) -> str:
        p = self.params
        return (
            f"{p['label']:<16} volume {self.stats['volume']:>10,.1f} mm^3  "
            f"faces {self.stats['faces']:>3}  "
            f"mass {p.get('mass_g', 0.0):>6.1f} g"
        )


def _finish(shape, label, params) -> PaddlePart:
    stats = verify(shape, label=label)
    params = dict(params)
    params.setdefault("label", label)
    # 1.24 g/cm^3 is a fair average for PLA / PETG
    params["mass_g"] = stats["volume"] / 1000.0 * 1.24
    return PaddlePart(shape, stats, params)


# ------------------------------------------------------------- profiles ----

def _head_points(spec: PaddleSpec, scale: float = 1.0) -> list:
    """The blade head outline: an egg, nearly flat along the bottom.

    Hand-placed points fed to a spline, so the head reads as a blade and not
    as a rounded rectangle.  `scale` shrinks the whole outline about the
    head's centre, which is how the rubber facing is fitted to it.
    """
    w = spec.head_w / 2.0 * scale
    h = spec.head_h * scale
    cy = spec.head_h * (1.0 - scale) / 2.0
    raw = [
        (0.00, 0.00),
        (0.42, 0.02),
        (0.72, 0.11),
        (0.90, 0.30),
        (0.99, 0.55),
        (0.93, 0.78),
        (0.70, 0.93),
        (0.36, 0.99),
        (0.00, 1.00),
        (-0.36, 0.99),
        (-0.70, 0.93),
        (-0.93, 0.78),
        (-0.99, 0.55),
        (-0.90, 0.30),
        (-0.72, 0.11),
        (-0.42, 0.02),
    ]
    return [(fx * w, fy * h + cy) for fx, fy in raw]


def _head_solid(spec: PaddleSpec, z0: float, z1: float) -> cq.Workplane:
    """The blade head: an egg-shaped slab whose bottom sits on y = 0."""
    pts = _head_points(spec)
    return (
        cq.Workplane("XY")
        .spline(pts + [pts[0]])
        .close()
        .extrude(z1 - z0)
        .translate((0.0, 0.0, z0))
    )


def _neck_solid(spec: PaddleSpec, z0: float, z1: float) -> cq.Workplane:
    """The neck: a straight tang running down from the head."""
    w = spec.neck_w / 2.0
    top = 12.0
    flare_y = -19.0
    pts = [
        (-spec.neck_top_w / 2.0, top),
        (spec.neck_top_w / 2.0, top),
        (w, flare_y),
        (w, -spec.neck_len),
        (-w, -spec.neck_len),
        (-w, flare_y),
    ]
    return (
        cq.Workplane("XY")
        .polyline(pts)
        .close()
        .extrude(z1 - z0)
        .translate((0.0, 0.0, z0))
        .edges("|Z and <Y")
        .fillet(spec.neck_r)
    )


def _tongue_profile(spec: PaddleSpec, grow: float):
    """The (x, z) outline of the dovetail ridge, or of its groove.

    The ridge tapers from `tongue_mouth` wide at the root to `tongue_tip`
    wide at the tip, and it is WIDE at the tip, so it locks.  `grow` widens
    the whole profile by that much on each side - that is the print
    clearance used for the groove.
    """
    m = spec.tongue_mouth / 2.0 + grow
    t = spec.tongue_tip / 2.0 + grow
    d = spec.tongue_depth
    over = 0.3                     # the root pokes up into the plate
    rate = (t - m) / (d + over)
    root = m - rate * over
    return [
        (-root, over),
        (root, over),
        (t, -d),
    ]


def _prism_xz(points, y0: float, y1: float) -> cq.Workplane:
    """A prism whose profile lives in the XZ plane and which runs along Y."""
    length = y1 - y0
    return (
        cq.Workplane("XZ")
        .polyline(points)
        .close()
        .extrude(-length)
        .translate((0.0, y0, 0.0))
    )


def _clamp_points(spec: PaddleSpec) -> list:
    """The 2D outline of a handle clamp: a flared grip with a wide collar."""
    y_top = spec.handle_top
    y_bot = y_top - spec.handle_len
    collar = spec.neck_w / 2.0 + spec.fit + 3.5
    return [
        (0.0, y_bot),
        (9.0, y_bot + 2.5),
        (14.0, y_bot + 9.0),
        (15.5, y_bot + 18.0),
        (13.5, y_bot + 30.0),
        (13.5, y_bot + 46.0),
        (14.5, y_bot + 58.0),
        (19.0, y_bot + 64.0),
        (collar, y_bot + 68.0),
        (collar, y_top - 3.0),
        (collar - 3.0, y_top - 0.5),
        (collar - 8.0, y_top),
        (0.0, y_top),
        (-collar + 8.0, y_top),
        (-collar + 3.0, y_top - 0.5),
        (-collar, y_top - 3.0),
        (-collar, y_bot + 68.0),
        (-19.0, y_bot + 64.0),
        (-14.5, y_bot + 58.0),
        (-13.5, y_bot + 46.0),
        (-13.5, y_bot + 30.0),
        (-15.5, y_bot + 18.0),
        (-14.0, y_bot + 9.0),
        (-9.0, y_bot + 2.5),
    ]


def _clamp_solid(spec: PaddleSpec, z0: float, z1: float) -> cq.Workplane:
    """One handle clamp, extruded over the given z range."""
    pts = _clamp_points(spec)
    return (
        cq.Workplane("XY")
        .spline(pts + [pts[0]])
        .close()
        .extrude(z1 - z0)
        .translate((0.0, 0.0, z0))
    )


def _hole_pair(spec: PaddleSpec, z0: float, z1: float, dia: float) -> cq.Workplane:
    """The two M3 holes, as one cutter."""
    h = (
        cq.Workplane("XY")
        .center(spec.screw_x, spec.screw_y)
        .circle(dia / 2.0)
        .extrude(z1 - z0)
        .translate((0.0, 0.0, z0))
    )
    return h.union(
        cq.Workplane("XY")
        .center(-spec.screw_x, spec.screw_y)
        .circle(dia / 2.0)
        .extrude(z1 - z0)
        .translate((0.0, 0.0, z0))
    )


# ------------------------------------------------------------ blade half ---

def blade_half(
    which: str = "front",
    spec: PaddleSpec | None = None,
) -> PaddlePart:
    """One half of the blade.

    which  "front" gives blade half 1 (the plate plus the dovetail RIDGE),
           "back"  gives blade half 2 (the plate minus the dovetail GROOVE).
    """
    spec = (spec or PaddleSpec()).validate()
    if which not in ("front", "back"):
        raise ValueError(f"which must be 'front' or 'back', got {which!r}")

    half = spec.half_t

    if which == "front":
        plate = _head_solid(spec, 0.0, half).union(_neck_solid(spec, 0.0, half))
        ridge = _prism_xz(
            _tongue_profile(spec, 0.0),
            spec.tongue_y0,
            spec.tongue_y1,
        )
        shape = plate.union(ridge)
        label = "blade_half_1"
    else:
        plate = _head_solid(spec, -half, 0.0).union(_neck_solid(spec, -half, 0.0))
        profile = _tongue_profile(spec, spec.fit)
        root_x, tip_x = profile[0][0], profile[-1][0]
        groove = _prism_xz(
            profile + [
                (tip_x, -half - 1.0),
                (-tip_x, -half - 1.0),
                (-root_x, -half - 1.0),
            ],
            spec.tongue_y0,
            spec.tongue_y1,
        )
        shape = plate.cut(groove)
        label = "blade_half_2"

    # the two M3 holes go through the solid part of the neck, below the
    # dovetail, where both halves have real material
    shape = shape.cut(
        _hole_pair(spec, -spec.blade_t, spec.blade_t, spec.screw_d)
    )

    params = dict(
        which=which,
        thickness=half,
        tongue_mouth=spec.tongue_mouth,
        tongue_tip=spec.tongue_tip,
        tongue_depth=spec.tongue_depth,
    )
    return _finish(shape, label, params)


# ---------------------------------------------------------- handle clamp ---

def handle_clamp(
    which: str = "front",
    spec: PaddleSpec | None = None,
) -> PaddlePart:
    """One half of the handle.

    which  "front" gives handle clamp 1 (the screw head side),
           "back"  gives handle clamp 2 (the nut side).
    """
    spec = (spec or PaddleSpec()).validate()
    if which not in ("front", "back"):
        raise ValueError(f"which must be 'front' or 'back', got {which!r}")

    t = spec.clamp_t
    shape = _clamp_solid(spec, 0.0, t) if which == "front" else _clamp_solid(spec, -t, 0.0)

    # the channel that receives the neck: it eats the inner half of the
    # clamp over the full length of the tang
    chan_w = spec.neck_w + 2 * spec.fit
    chan_y0 = -spec.neck_len
    chan_y1 = spec.handle_top + 0.5
    if which == "front":
        inner, outer = 0.0, spec.half_t
    else:
        inner, outer = -spec.half_t, 0.0
    channel = (
        cq.Workplane("XY")
        .box(chan_w, chan_y1 - chan_y0, outer - inner)
        .translate((0.0, (chan_y0 + chan_y1) / 2.0, (inner + outer) / 2.0))
    )
    shape = shape.cut(channel)

    # counterbore + dish, both cut into the OUTER face
    face_z = t if which == "front" else -t
    for x in (spec.screw_x, -spec.screw_x):
        cbore = (
            cq.Workplane("XY")
            .center(x, spec.screw_y)
            .circle(spec.cbore_d / 2.0)
            .extrude(spec.cbore_depth + 1.0)
            .translate((0.0, 0.0, face_z - spec.cbore_depth))
        )
        dish = (
            cq.Workplane("XY")
            .center(x, spec.screw_y)
            .circle(spec.dish_d / 2.0)
            .extrude(spec.dish_depth + 1.0)
            .translate((0.0, 0.0, face_z - spec.dish_depth))
        )
        shape = shape.cut(cbore).cut(dish)

    shape = shape.cut(_hole_pair(spec, -2 * t, 2 * t, spec.screw_d))

    params = dict(
        which=which,
        thickness=t,
        cbore_d=spec.cbore_d,
        cbore_depth=spec.cbore_depth,
        dish_d=spec.dish_d,
        dish_depth=spec.dish_depth,
    )
    label = "handle_clamp_1" if which == "front" else "handle_clamp_2"
    return _finish(shape, label, params)


# ------------------------------------------------------------- the rubber ---

def rubber_sheet(spec: PaddleSpec | None = None) -> PaddlePart:
    """One rubber facing, sized to sit on the blade head."""
    spec = (spec or PaddleSpec()).validate()
    scale = (spec.head_w - 2 * spec.rubber_inset) / spec.head_w
    pts = _head_points(spec, scale)
    shape = (
        cq.Workplane("XY")
        .spline(pts + [pts[0]])
        .close()
        .extrude(spec.rubber_t)
    )
    return _finish(shape, "rubber_sheet", dict(thickness=spec.rubber_t))


# ---------------------------------------------------------------- screws ----

def m3_screw(spec: PaddleSpec | None = None) -> PaddlePart:
    """A reference M3 socket head cap screw, drawn in place.

    It is not a printed part - it is here so the counterbores and the grip
    can be checked against a real screw head.
    """
    spec = (spec or PaddleSpec()).validate()
    t = spec.clamp_t
    head_z = t - spec.cbore_depth
    shank_len = head_z + t + 2.0
    head = (
        cq.Workplane("XY")
        .center(spec.screw_x, spec.screw_y)
        .circle(spec.cbore_d / 2.0 - 0.25)
        .extrude(3.0)
        .translate((0.0, 0.0, head_z))
    )
    shank = (
        cq.Workplane("XY")
        .center(spec.screw_x, spec.screw_y)
        .circle((spec.screw_d - 0.2) / 2.0)
        .extrude(shank_len)
        .translate((0.0, 0.0, head_z - shank_len))
    )
    shape = head.union(shank)
    return _finish(
        shape, "m3_screw", dict(diameter=spec.screw_d, shank_len=shank_len)
    )


# ------------------------------------------------------------- assembly ----

def paddle_assembly(spec: PaddleSpec | None = None) -> dict:
    """Every part of the paddle, keyed by name."""
    spec = (spec or PaddleSpec()).validate()
    return {
        "blade_half_1": blade_half("front", spec),
        "blade_half_2": blade_half("back", spec),
        "handle_clamp_1": handle_clamp("front", spec),
        "handle_clamp_2": handle_clamp("back", spec),
        "rubber_sheet": rubber_sheet(spec),
        "m3_screw": m3_screw(spec),
    }
