"""Meituan LongCat logo — a 3D-printable rendition of the cat-head mark.

The mark is the LongCat cat-head: an M-shaped outline (the two outer edges
rise from the ground to the ear tips, with a V notch between the ears), a
rounded cat-face inside, two triangular ears, and two slanted eyes.

The silhouette is taken directly from the LongCat logo SVG
(``longcat/yeqian-logo.svg``, green ``#29E154`` path), so the proportions are
exact rather than eyeballed.

Three ways to build it, all sharing the same source outlines:

    longcat_logo()      one flat plate; face and eyes are recessed pockets
    longcat_logo_split  three flat pieces (M shell / head / eyes) that slot
                        together, for printing in three colours
    longcat_logo_3d     the same three pieces, but the head is a bulging solid
                        that rises out of the shell (arc - straight - arc side
                        profile, eyes flush with the head surface)

    from examples.longcat_logo import longcat_logo, longcat_logo_split, longcat_logo_3d

    longcat_logo().save("logo.stl")

    parts = longcat_logo_split()
    for name, part in parts.items():
        part.save(f"{name}.stl")

    parts = longcat_logo_3d()
    parts["cat_head"].save("cat_head.stl")
"""
from __future__ import annotations

import math
import re
import zipfile
from dataclasses import dataclass
from pathlib import Path

import cadquery as cq
from shapely.geometry import Polygon

from pipeline.core import verify

__all__ = [
    "LogoSpec",
    "SplitSpec",
    "HeadSpec",
    "longcat_logo",
    "longcat_logo_info",
    "longcat_logo_split",
    "longcat_logo_3d",
    "save_split",
    "save_split_3mf",
]

# ------------------------------------------------------------------ source --
# Official logo paths (SVG path data, viewBox 0 0 40 40, y grows downward).
_OUTER_D = (
    "M3.7180590000000002,31.9163"
    "C3.24403,31.9163,2.9002264,31.4649,3.0262264,31.0079"
    "L9.07884,9.056280000000001"
    "C9.337579999999999,8.117891,10.43555,7.703758,11.24956,8.237532"
    "L19.2136,13.459869999999999"
    "C19.6909,13.77281,20.3081,13.77329,20.7859,13.46111"
    "L28.7837,8.234667"
    "C29.5984,7.702255,30.6956,8.11792,30.953,9.056519999999999"
    "L36.974,31.0089"
    "C37.0993,31.4656,36.7556,31.9163,36.2819,31.9163"
    "L28.5948,31.9163"
    "C29.9941,30.2961,30.7641,28.2266,30.7641,26.0857"
    "L30.7641,25.8358"
    "C30.7641,23.7417,30.0023,21.719,28.6209,20.1451"
    "L27.6344,15.19322"
    "C27.5764,14.90227,27.321,14.69275,27.0243,14.69275"
    "C26.8898,14.69275,26.7588,14.7364,26.6511,14.81715"
    "L22.9316,17.60681"
    "C22.6667,17.80548,22.3241,17.868740000000003,22.0057,17.77777"
    "C20.6944,17.403100000000002,19.3043,17.403100000000002,17.9929,17.77777"
    "C17.674599999999998,17.868740000000003,17.332,17.80548,17.0671,17.60681"
    "L13.3459,14.815909999999999"
    "C13.2393,14.73596,13.1096,14.69275,12.97639,14.69275"
    "C12.67948,14.69275,12.42483,14.90461,12.37083,15.196570000000001"
    "L11.41369,20.371000000000002"
    "C10.01719,21.7911,9.2346,23.703,9.2346,25.6947"
    "L9.2346,26.168"
    "C9.2346,28.2566,9.98171,30.2762,11.3409,31.8619"
    "L11.38755,31.9163"
    "L3.7180590000000002,31.9163Z"
)
_EYES_D = (
    "M16.05224895477295,27.610614743041992L18.20519895477295,27.610614743041992"
    "L18.20519895477295,22.587064743041992L16.37845295477295,22.587064743041992"
    "L16.05224895477295,27.610614743041992Z"
    "M23.94638895477295,27.610614743041992L21.79344895477295,27.610614743041992"
    "L21.79344895477295,22.587064743041992L23.62018895477295,22.587064743041992"
    "L23.94638895477295,27.610614743041992Z"
)
_SAMPLES = 24  # curve subdivisions per Bezier segment


# ----------------------------------------------------------------- parsing --
def _parse_path(d: str, samples: int = _SAMPLES) -> list[list[tuple[float, float]]]:
    """Parse SVG path data (M/L/C/Q/Z) into a list of closed subpaths."""
    tokens = re.findall(r"[MLCQZ]|-?\d*\.?\d+(?:[eE][-+]?\d+)?", d)
    subpaths: list[list[tuple[float, float]]] = []
    cur: list[tuple[float, float]] = []
    cmd = ""
    i = 0
    while i < len(tokens):
        t = tokens[i]
        if t.isalpha():
            cmd = t
            i += 1
            if cmd == "Z" and cur:
                subpaths.append(cur)
                cur = []
            continue
        if cmd in ("M", "L"):
            x, y = float(t), float(tokens[i + 1])
            i += 2
            if cmd == "M":
                if cur:
                    subpaths.append(cur)
                cur = [(x, y)]
            else:
                cur.append((x, y))
        elif cmd == "C":
            x1, y1, x2, y2, x, y = (float(tokens[i + k]) for k in range(6))
            i += 6
            p0 = cur[-1]
            for k in range(1, samples + 1):
                s = k / samples
                cur.append((
                    (1 - s) ** 3 * p0[0] + 3 * (1 - s) ** 2 * s * x1
                    + 3 * (1 - s) * s * s * x2 + s ** 3 * x,
                    (1 - s) ** 3 * p0[1] + 3 * (1 - s) ** 2 * s * y1
                    + 3 * (1 - s) * s * s * y2 + s ** 3 * y,
                ))
        elif cmd == "Q":
            x1, y1, x, y = (float(tokens[i + k]) for k in range(4))
            i += 4
            p0 = cur[-1]
            for k in range(1, samples + 1):
                s = k / samples
                cur.append((
                    (1 - s) ** 2 * p0[0] + 2 * (1 - s) * s * x1 + s * s * x,
                    (1 - s) ** 2 * p0[1] + 2 * (1 - s) * s * y1 + s * s * y,
                ))
        else:
            i += 1
    if cur:
        subpaths.append(cur)
    return subpaths


def _split_keyhole(pts: list[tuple[float, float]]) -> tuple[list, list]:
    """Split the logo's single keyhole path into (outer, inner) loops.

    The path traces the outer M, runs along the bottom to the inner face
    opening, traces it, and runs back.  Both bridges sit at the maximum y.
    """
    ymax = max(y for _, y in pts)
    idx = [i for i, (_, y) in enumerate(pts) if abs(y - ymax) < 1e-6]
    if len(idx) < 4:
        raise ValueError("unexpected logo path: could not find the bottom bridges")
    outer = pts[idx[0]: idx[1] + 1]
    inner = pts[idx[2]: idx[3] + 1]
    return outer, inner


def _logo_polygons() -> tuple[Polygon, Polygon, list[Polygon]]:
    """Return (outer M, inner face, [eye, eye]) in print coordinates (y up)."""
    outer_pts, inner_pts = _split_keyhole(_parse_path(_OUTER_D)[0])
    eyes = _parse_path(_EYES_D)

    ymax = max(y for _, y in outer_pts)

    def up(pts):
        return [(x, ymax - y) for x, y in pts]

    outer = Polygon(up(outer_pts))
    inner = Polygon(up(inner_pts))
    eye_polys = [Polygon(up(p)) for p in eyes]
    return outer, inner, eye_polys


def _poly_wp(poly: Polygon, scale: float, height: float) -> cq.Workplane:
    """Extrude a shapely polygon (already y-up) into a scaled CadQuery solid."""
    coords = [(x * scale, y * scale) for x, y in poly.exterior.coords[:-1]]
    return cq.Workplane("XY").polyline(coords).close().extrude(height)


# --------------------------------------------------------------- parameters --
@dataclass
class LogoSpec:
    """Print parameters for the monolithic Meituan LongCat logo, in millimetres."""

    thickness: float = 5.0     # print thickness (Z)
    scale: float = 2.0         # scale factor applied to the 40x40 source art
    face_pocket: float = 1.5   # depth of the cat-face recess
    eye_pocket: float = 3.0    # depth of the eye recess

    def validate(self) -> None:
        if self.thickness <= 0:
            raise ValueError("thickness must be > 0")
        if self.scale <= 0:
            raise ValueError("scale must be > 0")
        if not 0 < self.face_pocket < self.thickness:
            raise ValueError("face_pocket must be between 0 and thickness")
        if not 0 < self.eye_pocket < self.thickness:
            raise ValueError("eye_pocket must be between 0 and thickness")

    @property
    def width(self) -> float:
        b = _logo_polygons()[0].bounds
        return (b[2] - b[0]) * self.scale

    @property
    def height(self) -> float:
        b = _logo_polygons()[0].bounds
        return (b[3] - b[1]) * self.scale


@dataclass
class SplitSpec:
    """Print parameters for the split (three-colour) Meituan LongCat logo."""

    thickness: float = 5.0   # print thickness (Z), same for every part
    scale: float = 2.0       # scale factor applied to the 40x40 source art
    clearance: float = 0.0   # optional gap between mating parts (0 = snug)

    def validate(self) -> None:
        if self.thickness <= 0:
            raise ValueError("thickness must be > 0")
        if self.scale <= 0:
            raise ValueError("scale must be > 0")
        if self.clearance < 0:
            raise ValueError("clearance must be >= 0")

    @property
    def width(self) -> float:
        b = _logo_polygons()[0].bounds
        return (b[2] - b[0]) * self.scale

    @property
    def height(self) -> float:
        b = _logo_polygons()[0].bounds
        return (b[3] - b[1]) * self.scale


@dataclass
class HeadSpec:
    """Print parameters for the 3D (bulging-head) Meituan LongCat logo."""

    thickness: float = 5.0    # flat M shell thickness (Z) == head rim width
    scale: float = 2.0        # scale factor applied to the 40x40 source art
    front_h: float = 6.0      # how far the head bulges above the shell top
    back_h: float = 5.0       # how far the head bulges below the shell bottom
    eye_depth: float = 1.5    # how deep the eye pockets are cut into the head

    def validate(self) -> None:
        if self.thickness <= 0:
            raise ValueError("thickness must be > 0")
        if self.scale <= 0:
            raise ValueError("scale must be > 0")
        if self.front_h <= 0:
            raise ValueError("front_h must be > 0")
        if self.back_h <= 0:
            raise ValueError("back_h must be > 0")
        if self.eye_depth <= 0:
            raise ValueError("eye_depth must be > 0")

    @property
    def width(self) -> float:
        b = _logo_polygons()[0].bounds
        return (b[2] - b[0]) * self.scale

    @property
    def height(self) -> float:
        b = _logo_polygons()[0].bounds
        return (b[3] - b[1]) * self.scale


# ------------------------------------------------------- monolithic (flat) --
def longcat_logo(spec: LogoSpec | None = None) -> cq.Workplane:
    """Build the Meituan LongCat logo as a single watertight solid.

    One flat plate (XY = the flat face, +Z = print thickness); the face and
    eyes are pockets on the front face, so colour zones come from a filament
    swap or painting rather than separate parts.
    """
    spec = LogoSpec() if spec is None else spec
    spec.validate()

    outer, inner, eyes = _logo_polygons()

    solid = _poly_wp(outer, spec.scale, spec.thickness)
    face = _poly_wp(inner, spec.scale, spec.thickness).translate(
        (0, 0, spec.thickness - spec.face_pocket)
    )
    solid = solid.cut(face)

    for eye in eyes:
        e = _poly_wp(eye, spec.scale, spec.thickness).translate(
            (0, 0, spec.thickness - spec.eye_pocket)
        )
        solid = solid.cut(e)

    verify(solid, label="longcat_logo")
    return solid


def longcat_logo_info(spec: LogoSpec | None = None) -> str:
    """One-line description of the built emblem, for quick checks."""
    spec = LogoSpec() if spec is None else spec
    shp = longcat_logo(spec).val()
    mass = shp.Volume() * 1.24e-3
    return (
        f"longcat_logo  {spec.width:.1f}x{spec.height:.1f}x"
        f"{spec.thickness:.1f} mm  volume {shp.Volume():,.1f} mm^3  "
        f"~{mass:.1f} g (PLA)"
    )


# ------------------------------------------------------------ split (flat) --
def longcat_logo_split(spec: SplitSpec | None = None) -> dict[str, cq.Workplane]:
    """Build the three flat colour components of the Meituan LongCat logo.

    Returns an ordered dict of ``{name: cq.Workplane}``:
    ``m_shell``, ``cat_head``, ``eyes``.  All three are authored at the same Z
    (0..thickness) and share the exact source outlines, so the assembled set is
    identical to the monolithic emblem.
    """
    spec = SplitSpec() if spec is None else spec
    spec.validate()

    outer, inner, eyes = _logo_polygons()
    s = spec.scale
    c = spec.clearance / 2.0  # half-gap: shrink the inlay, grow the hole

    outer_wp = _poly_wp(outer, s, spec.thickness)
    inner_wp = _poly_wp(inner.buffer(c) if c else inner, s, spec.thickness)

    head_poly = inner.buffer(-c) if c else inner
    head_holes = [e.buffer(c) if c else e for e in eyes]
    eye_polys = [e.buffer(-c) if c else e for e in eyes]

    m_shell = outer_wp.cut(inner_wp)

    cat_head = _poly_wp(head_poly, s, spec.thickness)
    for h in head_holes:
        cat_head = cat_head.cut(_poly_wp(h, s, spec.thickness))

    eye_wps = [_poly_wp(e, s, spec.thickness) for e in eye_polys]
    # each eye is verified on its own (the two are disjoint, so the pair
    # would fail verify's "exactly one solid" rule)
    for i, e in enumerate(eye_wps):
        verify(e, label=f"longcat_logo_split:eye_{i}")

    eyes_part = eye_wps[0]
    for e in eye_wps[1:]:
        eyes_part = eyes_part.union(e)
    verify(eyes_part, require_solid=False, label="longcat_logo_split:eyes")

    return {"m_shell": m_shell, "cat_head": cat_head, "eyes": eyes_part}


# ------------------------------------------------------------- 3D (domed) --
def _ellipsoid(cx: float, cy: float, cz: float,
               rx: float, ry: float, rz: float, n: int = 40) -> cq.Workplane:
    """A general ellipsoid (three independent semi-axes) at (cx, cy, cz).

    Built as a loft of ellipses rather than a transformed sphere: OCCT's sphere
    tessellation leaves a 4-edge crack at the pole, so an exported STL would not
    be watertight.  A loft of closed wires is exactly watertight.

    Use this when a smooth cap is wanted and the base may be larger than the
    target outline; use :func:`_dome` when the rim must equal the outline exactly.
    """
    wires = []
    for i in range(n + 1):
        t = -1.0 + 2.0 * i / n
        f = max(math.sqrt(max(0.0, 1.0 - t * t)), 0.02)
        wires.append(
            cq.Workplane("XY").workplane(offset=cz + rz * t)
            .ellipse(rx * f, ry * f).val()
        )
    solid = cq.Solid.makeLoft(wires, ruled=False)
    return cq.Workplane("XY").newObject([solid]).translate((cx, cy, 0))


def _dome(raw, cx, cy, z0, h, sections: int = 14, down: bool = False) -> cq.Workplane:
    """Loft the face outline to a point; the dome rim sits exactly at z0.

    Unlike an ellipsoid (whose base is larger than the outline), this dome's
    rim is the outline itself at z=z0, so the straight rim of the head is
    exactly the shell thickness.

    down=False -> the outline is at z0 and the apex at z0 + h (front bulge).
    down=True  -> the outline is at z0 and the apex at z0 - h (back bulge).
    """
    sign = -1.0 if down else 1.0
    wp = cq.Workplane("XY").polyline(raw).close()
    prev = 0.0
    for i in range(1, sections + 1):
        t = i / sections
        z = sign * h * t
        scale = math.sqrt(max(0.0, 1.0 - t * t))
        dz = z - prev
        if scale < 1e-3:
            wp = wp.workplane(offset=dz).center(cx, cy).circle(0.05)
        else:
            scaled = [(cx + (x - cx) * scale, cy + (y - cy) * scale) for x, y in raw]
            wp = wp.workplane(offset=dz).polyline(scaled).close()
        prev = z
    return wp.loft(ruled=False).translate((0, 0, z0))


def _polygon_head(poly, scale: float, thickness: float, front_h: float,
                  back_h: float) -> cq.Workplane:
    """Cat head: flat rim through the shell, domed front and back.

    The side profile is arc - straight - arc: the straight rim at the edge is
    exactly `thickness` wide (the M shell width), and the two arcs bulge freely.
    """
    raw = [(x * scale, y * scale) for x, y in poly.exterior.coords[:-1]]
    cx, cy = poly.centroid.x * scale, poly.centroid.y * scale

    prism = cq.Workplane("XY").polyline(raw).close().extrude(thickness)
    front = _dome(raw, cx, cy, thickness, front_h)
    back = _dome(raw, cx, cy, 0.0, back_h, down=True)
    return prism.union(front).union(back)


def longcat_logo_3d(spec: HeadSpec | None = None) -> dict[str, cq.Workplane]:
    """Build the 3D emblem: flat M shell + bulging head + flush quad eyes.

    The head rises out of the shell opening with its ears/notch intact.  Its
    side profile is arc - straight - arc, the straight rim being exactly the
    shell width.  The eyes are cut into the head and the eye pieces fill the
    pockets, so they sit flush with the head surface.
    """
    spec = HeadSpec() if spec is None else spec
    spec.validate()

    outer, inner, eyes = _logo_polygons()
    s = spec.scale
    T = spec.thickness

    m_shell = _poly_wp(outer, s, T).cut(_poly_wp(inner, s, T))

    head = _polygon_head(inner, s, T, spec.front_h, spec.back_h)

    # eyes: pockets cut into the head; the eye piece fills them flush
    head_orig = head
    eye_cuts: list[cq.Workplane] = []
    for e in eyes:
        coords = [(x * s, y * s) for x, y in e.exterior.coords[:-1]]
        eye_cuts.append(
            cq.Workplane("XY").polyline(coords).close()
            .extrude(spec.front_h + T + 1.0)
        )

    for ec in eye_cuts:
        head = head.cut(ec)

    eye_parts = [ec.intersect(head_orig) for ec in eye_cuts]
    eyes_part = eye_parts[0]
    for e in eye_parts[1:]:
        eyes_part = eyes_part.union(e)

    verify(m_shell, label="longcat_logo_3d:m_shell")
    verify(head, label="longcat_logo_3d:cat_head")
    verify(eyes_part, require_solid=False, label="longcat_logo_3d:eyes")

    return {"m_shell": m_shell, "cat_head": head, "eyes": eyes_part}


# ------------------------------------------------------------------ export --
def save_split(
    parts: dict[str, cq.Workplane],
    outdir: str | Path = "out/longcat_split",
) -> list[Path]:
    """Write one STL per component and return the paths."""
    outdir = Path(outdir)
    outdir.mkdir(parents=True, exist_ok=True)
    written: list[Path] = []
    for name, part in parts.items():
        p = outdir / f"longcat_{name}.stl"
        cq.exporters.export(part, str(p))
        written.append(p)
    return written


def _model_xml(parts: dict[str, cq.Workplane]) -> str:
    """Assemble a minimal 3MF model with one object per component."""
    from pipeline.three_mf import _cq_to_trimesh, _mesh_xml

    objects: list[str] = []
    items: list[str] = []
    for i, (name, part) in enumerate(parts.items(), start=1):
        mesh = _cq_to_trimesh(part)
        objects.append(
            f'    <object id="{i}" type="model">\n'
            f'      <metadatagroup>\n'
            f'        <metadata name="Name" type="xs:string">{name}</metadata>\n'
            f'      </metadatagroup>\n'
            f'{_mesh_xml(mesh, indent="      ")}\n'
            f'    </object>\n'
        )
        items.append(
            f'    <item objectid="{i}" '
            f'transform="1 0 0 0 0 1 0 0 0 0 1 0" printable="1"/>\n'
        )

    return (
        '<?xml version="1.0" encoding="UTF-8"?>\n'
        '<model unit="millimeter" xml:lang="en-US" '
        'xmlns="http://schemas.microsoft.com/3dmanufacturing/core/2015/02">\n'
        '  <metadata name="Application">opencode longcat_logo</metadata>\n'
        '  <metadata name="Title">LongCat logo (split, 3 colours)</metadata>\n'
        '  <resources>\n'
        + "".join(objects) +
        '  </resources>\n'
        '  <build>\n'
        + "".join(items) +
        '  </build>\n'
        '</model>\n'
    )


_CONTENT_TYPES = (
    '<?xml version="1.0" encoding="UTF-8"?>\n'
    '<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">\n'
    '  <Default Extension="rels" '
    'ContentType="application/vnd.openxmlformats-package.relationships+xml"/>\n'
    '  <Default Extension="model" '
    'ContentType="application/vnd.ms-package.3dmanufacturing-3dmodel+xml"/>\n'
    '</Types>\n'
)

_RELS = (
    '<?xml version="1.0" encoding="UTF-8"?>\n'
    '<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">\n'
    '  <Relationship Target="/3D/3dmodel.model" Id="rel0" '
    'Type="http://schemas.microsoft.com/3dmanufacturing/2013/01/3dmodel"/>\n'
    '</Relationships>\n'
)


def save_split_3mf(
    parts: dict[str, cq.Workplane],
    path: str | Path = "out/longcat_split/longcat_logo_3colours.3mf",
) -> Path:
    """Write the components as a 3-object 3MF for the slicer's colour picker."""
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    model = _model_xml(parts)
    with zipfile.ZipFile(path, "w", zipfile.ZIP_DEFLATED) as z:
        z.writestr("[Content_Types].xml", _CONTENT_TYPES)
        z.writestr("_rels/.rels", _RELS)
        z.writestr("3D/3dmodel.model", model)
    return path
