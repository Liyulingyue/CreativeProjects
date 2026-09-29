"""three_mf - dual-engine round-trip for Bambu / Orca 3MF files.

Why this module exists
----------------------
A Bambu Studio / OrcaSlicer .3mf is a ZIP that carries far more than
meshes.  Inside it sit

    3D/3dmodel.model          geometry (XML)
    3D/Objects/Object_N.model optional per-object mesh files
    Metadata/project_settings.config  print profiles
    Metadata/slice_info.config         slice overrides
    Metadata/colors.xml                per-vertex paint
    thumbnail.png
    ... and other slicer-specific XML

A naive ``trimesh.Scene.export("out.3mf")`` writes a technically correct
3MF, but it discards every one of those files and Bambu Studio sees the
result as a brand-new project with no paint, no profile, no plate
layout.  The round-trip is therefore:

    1. OPEN  the original ZIP, keep every byte in memory.
    2. READ  the geometry with trimesh, only so we can get a bounding box.
    3. BUILD the new solid with CadQuery (the existing grid_panel /
       ribbed_plate generators feed in here).
    4. SPLICE the new mesh XML into the original 3dmodel.model, replacing
       exactly one <object>'s <mesh> block, byte for byte outside of it.
    5. REPACK into a new ZIP, copying every original entry across and
       overwriting only the file(s) we touched.

This module is therefore trimesh (read) + CadQuery (CSG) + zipfile
(round-trip).  No lib3mf / no Py3MF / no trimesh.Scene.export.  Each
engine does the one thing it is good at.

Usage
-----
    from generators import three_mf, grid_panel

    bundle = three_mf.load("input_bambu.3mf")
    info = bundle.get_part(object_id=1)             # trimesh.Trimesh
    length = info.bbox[1, 0] - info.bbox[0, 0]
    width  = info.bbox[1, 1] - info.bbox[0, 1]
    grid   = grid_panel(length, width, 3, rib=4, gap=5, direction="u")

    # Option A: replace the part's mesh in place
    bundle.replace_mesh(object_id=1, shape=grid,
                        save_as="output_bambu.3mf")

    # Option B: append a modifier mesh (Bambu will treat as Modifier)
    bundle.add_modifier(shape=grid, save_as="with_modifier.3mf")

    # Option C: union with the original (loses color painting, be careful)
    bundle.union_replace(object_id=1, shape=grid,
                         save_as="merged.3mf")
"""
from __future__ import annotations

import io
import re
import shutil
import tempfile
import zipfile
from dataclasses import dataclass, field
from pathlib import Path
from typing import Iterable

from typing import TYPE_CHECKING

import numpy as np
import trimesh

# CadQuery is only needed when one of the shape bridges is actually
# called (replace_mesh / add_modifier / union_replace).  Keeping the
# import lazy means the inspection and round-trip APIs work with
# trimesh alone, which is useful for CI smoke tests and for users who
# only want to read a Bambu 3MF.
if TYPE_CHECKING:
    import cadquery as cq


# --------------------------------------------------------------------- errors


class ThreeMFError(Exception):
    """Anything that goes wrong reading or rewriting a Bambu 3MF."""


# ---------------------------------------------------------------- mesh bridge


def _cq_to_trimesh(shape) -> trimesh.Trimesh:
    """CadQuery shape -> single watertight trimesh.Trimesh.

    Accepts a ``cq.Workplane``, a ``cq.Shape``, or one of our generator
    dataclasses (GridPanel, RibbedPlate).  CadQuery tessellates via OCCT
    on the C++ side; we round-trip through STL bytes because that is the
    most portable path (works with the official ``cadquery`` package on
    every platform).  The temp file is deleted before return so the
    working directory is never touched.
    """
    import cadquery as cq  # lazy: only needed when a shape is actually bridged

    # Unwrap generator dataclasses: GridPanel.shape / RibbedPlate.shape
    if hasattr(shape, "shape") and isinstance(getattr(shape, "shape", None),
                                              cq.Workplane):
        shape = shape.shape

    # Reject obviously-empty inputs before we hand them to CadQuery,
    # because CadQuery's error for an empty shape is unhelpful.
    if isinstance(shape, cq.Workplane):
        try:
            solids = shape.solids().vals()
        except Exception:
            solids = None
        if solids is not None and len(solids) == 0:
            raise ThreeMFError("CadQuery shape has no solids to export")

    with tempfile.TemporaryDirectory() as td:
        stl = Path(td) / "x.stl"
        try:
            cq.exporters.export(shape, str(stl), exportType="STL")
        except Exception as e:
            raise ThreeMFError(
                f"CadQuery failed to tessellate shape to STL: {e}"
            ) from e
        m = trimesh.load_mesh(str(stl), force="mesh")
    if isinstance(m, trimesh.Scene):
        parts = [g for g in m.geometry.values() if isinstance(g, trimesh.Trimesh)]
        if not parts:
            raise ThreeMFError("CadQuery shape tessellated to empty scene")
        if len(parts) == 1:
            return parts[0]
        return trimesh.util.concatenate(parts)
    return m


def _mesh_xml(mesh: trimesh.Trimesh, indent: str = "    ") -> str:
    """Emit a 3MF <mesh> block from a trimesh.

    Vertices and triangles are emitted in their order in mesh.vertices /
    mesh.faces, so indices in <triangle v1 v2 v3> refer to those
    positions directly.  Coordinates are formatted with 6 decimals which
    is well below Bambu Studio's internal precision (it stores f32).
    """
    v = np.asarray(mesh.vertices, dtype=np.float64)
    f = np.asarray(mesh.faces, dtype=np.int64)
    if v.size == 0 or f.size == 0:
        raise ThreeMFError("refusing to emit an empty <mesh>")

    lines = [f"{indent}<mesh>"]
    lines.append(f"{indent}  <vertices>")
    for x, y, z in v:
        lines.append(f'{indent}    <vertex x="{x:.6f}" y="{y:.6f}" z="{z:.6f}"/>')
    lines.append(f"{indent}  </vertices>")
    lines.append(f"{indent}  <triangles>")
    for a, b, c in f:
        lines.append(f'{indent}    <triangle v1="{int(a)}" v2="{int(b)}" v3="{int(c)}"/>')
    lines.append(f"{indent}  </triangles>")
    lines.append(f"{indent}</mesh>")
    return "\n".join(lines)


# ---------------------------------------------------------- XML splicing core


# Match a single <object id="N" ...>...</object> block.  The body
# capture is non-greedy and DOTALL so nested </object> tags in the
# body cannot accidentally close the match.  The id attribute is
# captured for sanity checking.
_OBJECT_RE = re.compile(
    rb'(<object\b[^>]*\bid="(?P<id>\d+)"[^>]*>)(.*?)(</object>)',
    re.DOTALL,
)

# Match a single <mesh ...>...</mesh> block (with or without attributes).
_MESH_RE = re.compile(rb'<mesh\b[^>]*>.*?</mesh>', re.DOTALL)

# Match a <resources>...</resources> block (where we insert new <object>s).
_RESOURCES_RE = re.compile(
    rb'(<resources\b[^>]*>)(.*?)(</resources>)',
    re.DOTALL,
)

# Match a <build>...</build> block.
_BUILD_RE = re.compile(rb'(<build\b[^>]*>)(.*?)(</build>)', re.DOTALL)

# Match an <item .../> inside <build>.  Self-closing only.
_ITEM_RE = re.compile(rb'<item\b[^/>]*/>')


def _splice_object_mesh(xml_bytes: bytes, object_id: int, new_mesh_xml: str) -> bytes:
    """Replace the <mesh>...</mesh> of <object id="N"> with new_mesh_xml.

    Bytes outside the replaced mesh block are returned verbatim, so
    Bambu's namespaces, comments, attribute order and whitespace are
    perfectly preserved.
    """
    pattern = re.compile(
        rb'<object\b[^>]*\bid="' + str(object_id).encode() + rb'"[^>]*>'
        rb'.*?</object>',
        re.DOTALL,
    )

    def _repl(match: re.Match) -> bytes:
        block = match.group(0)
        # locate first <mesh>...</mesh> inside the matched object block
        new_block, n = _MESH_RE.subn(new_mesh_xml.encode(), block, count=1)
        if n != 1:
            raise ThreeMFError(
                f"object id={object_id} found but has no <mesh> block "
                f"- this object is a component/assembly, not a mesh"
            )
        return new_block

    new_bytes, n = pattern.subn(_repl, xml_bytes, count=1)
    if n != 1:
        raise ThreeMFError(
            f"object id={object_id} not found in 3D/3dmodel.model"
        )
    return new_bytes


def _next_object_id(xml_bytes: bytes) -> int:
    """Return max(object id) + 1 in the model XML, or 1 if empty."""
    ids = [int(m) for m in re.findall(rb'<object\b[^>]*\bid="(\d+)"', xml_bytes)]
    return (max(ids) + 1) if ids else 1


def _strip_mesh(xml_bytes: bytes, object_id: int) -> bytes:
    """Drop the <mesh>...</mesh> block from <object id="N">, leaving it empty.

    Useful before adding a modifier mesh that should not collide with
    an existing one.
    """
    pattern = re.compile(
        rb'<object\b[^>]*\bid="' + str(object_id).encode() + rb'"[^>]*>'
        rb'.*?</object>',
        re.DOTALL,
    )

    def _repl(match: re.Match) -> bytes:
        block = match.group(0)
        new_block, n = _MESH_RE.subn(b"", block, count=1)
        if n != 1:
            raise ThreeMFError(f"object id={object_id} has no <mesh>")
        return new_block

    new_bytes, n = pattern.subn(_repl, xml_bytes, count=1)
    if n != 1:
        raise ThreeMFError(f"object id={object_id} not found")
    return new_bytes


def _insert_object_and_build_item(
    xml_bytes: bytes,
    new_object_xml: str,
    build_item_xml: str,
) -> bytes:
    """Insert a new <object> into <resources> and a new <item/> into <build>.

    Both insertions are byte-level; everything else stays untouched.
    """
    def _res(m: re.Match) -> bytes:
        return m.group(1) + m.group(2) + new_object_xml.encode() + m.group(3)

    res_match = _RESOURCES_RE.search(xml_bytes)
    if not res_match:
        raise ThreeMFError("3dmodel.model has no <resources> block")
    xml_bytes = _RESOURCES_RE.sub(_res, xml_bytes, count=1)

    def _bld(m: re.Match) -> bytes:
        return m.group(1) + m.group(2) + build_item_xml.encode() + m.group(3)

    bld_match = _BUILD_RE.search(xml_bytes)
    if not bld_match:
        raise ThreeMFError("3dmodel.model has no <build> block")
    xml_bytes = _BUILD_RE.sub(_bld, xml_bytes, count=1)
    return xml_bytes


# ---------------------------------------------------------------- public API


@dataclass
class PartInfo:
    """One leaf-object inside the loaded 3MF."""
    object_id: int
    name: str
    mesh: trimesh.Trimesh
    bbox: np.ndarray  # shape (2, 3): [mins, maxs]

    @property
    def size(self) -> tuple[float, float, float]:
        """(dx, dy, dz) along X, Y, Z in model units."""
        diff = self.bbox[1] - self.bbox[0]
        return float(diff[0]), float(diff[1]), float(diff[2])


@dataclass
class Bundle:
    """An open Bambu 3MF ready for in-place modification."""
    source_path: Path
    _entries: dict[str, bytes] = field(default_factory=dict)

    # ---- introspection --------------------------------------------------

    def list_parts(self) -> list[PartInfo]:
        """Return a PartInfo for every leaf object with a mesh.

        "Leaf" means the object has its own <mesh> block, not a
        <components> assembly.  Assemblies have no bounding box of
        their own and so are useless for sizing a CadQuery generator.
        """
        scene = trimesh.load(
            str(self.source_path),
            force="scene",
            process=False,
        )
        out: list[PartInfo] = []
        for name, geom in scene.geometry.items():
            if not isinstance(geom, trimesh.Trimesh):
                continue
            try:
                oid = int(name.split("/")[0].split(":")[0])
            except (ValueError, AttributeError):
                continue
            out.append(
                PartInfo(
                    object_id=oid,
                    name=name,
                    mesh=geom,
                    bbox=np.asarray(geom.bounds, dtype=np.float64),
                )
            )
        # also scan the underlying XML in case trimesh dropped an object
        xml = self._entries.get("3D/3dmodel.model", b"")
        xml_ids = {int(i) for i in re.findall(rb'<object\b[^>]*\bid="(\d+)"', xml)}
        for oid in xml_ids:
            if not any(p.object_id == oid for p in out):
                out.append(PartInfo(object_id=oid, name=str(oid),
                                    mesh=None, bbox=np.zeros((2, 3))))
        out.sort(key=lambda p: p.object_id)
        return out

    def get_part(self, object_id: int | None = None,
                 name: str | None = None) -> PartInfo:
        """Look up a part by id (preferred) or trimesh name."""
        parts = self.list_parts()
        if object_id is None and name is None:
            if not parts:
                raise ThreeMFError("no leaf parts in 3MF")
            return parts[0]
        for p in parts:
            if object_id is not None and p.object_id == object_id:
                return p
            if name is not None and p.name == name:
                return p
        raise ThreeMFError(
            f"part not found (object_id={object_id}, name={name})"
        )

    def bbox(self, object_id: int | None = None) -> np.ndarray:
        """Bounding box of one part, or AABB of all parts."""
        if object_id is None:
            parts = self.list_parts()
            meshes = [p.mesh for p in parts if p.mesh is not None]
            if not meshes:
                raise ThreeMFError("no meshes -> no bbox")
            stacked = np.vstack([m.bounds for m in meshes])
            return np.array([stacked.min(0), stacked.max(0)])
        return self.get_part(object_id=object_id).bbox

    # ---- modification --------------------------------------------------

    def replace_mesh(self, *, object_id: int, shape, save_as=None) -> "Bundle":
        """Replace the mesh of <object id=N> with a CadQuery solid.

        Bambu metadata (paint, profile, plate layout) is preserved.
        Returns a new Bundle pointing at the saved file.
        """
        new_mesh = _cq_to_trimesh(shape)
        new_xml = _mesh_xml(new_mesh)
        xml = self._entries["3D/3dmodel.model"]
        xml = _splice_object_mesh(xml, object_id, new_xml)
        return self._save_with(xml, save_as)

    def add_modifier(self, *, shape, save_as=None,
                     modifier_id: int | None = None,
                     translate: tuple[float, float, float] = (0.0, 0.0, 0.0)) -> "Bundle":
        """Append the shape as a brand-new modifier object.

        Bambu Studio will load it as a separate object; right-click its
        row in the part list and choose "Type -> Modifier" to make it
        act as a negative volume when slicing.  We emit ``type="other"``
        so Bambu Studio treats it as a candidate modifier.

        ``translate`` is a (dx, dy, dz) in mm, baked into the object's
        3MF transform so the modifier sits in the right spot relative
        to the printable parts (the modifier's own geometry is in its
        own local frame).
        """
        new_mesh = _cq_to_trimesh(shape)
        mesh_block = _mesh_xml(new_mesh)

        xml = self._entries["3D/3dmodel.model"]
        new_id = modifier_id if modifier_id is not None else _next_object_id(xml)

        tx, ty, tz = translate
        # 3MF transform: row-major 4x3 affine (the last row is implicit 0 0 0 1).
        transform = f"1 0 0 {tx} 0 1 0 {ty} 0 0 1 {tz}"

        new_object = (
            f'  <object id="{new_id}" type="other" '
            f'transform="{transform}">\n'
            f'{mesh_block}\n'
            f'  </object>\n'
        )
        build_item = (
            f'    <item objectid="{new_id}" transform="{transform}" '
            f'printable="0"/>\n'
        )
        xml = _insert_object_and_build_item(xml, new_object, build_item)
        return self._save_with(xml, save_as)

    def union_replace(self, *, object_id: int, shape, save_as=None) -> "Bundle":
        """Boolean union of the original part and a CadQuery solid.

        WARNING: boolean union rebuilds vertices and triangles from
        scratch.  Per-vertex color painting on the original mesh will be
        lost because the new mesh has no paint attributes.  If you care
        about paint, use add_modifier() instead - it preserves every
        byte of the original mesh.
        """
        original = self.get_part(object_id=object_id).mesh
        new = _cq_to_trimesh(shape)

        try:
            import trimesh.boolean as tb  # noqa: F401
            merged = tb.union([original, new], engine="manifold")
        except Exception:
            # Fallback: trimesh.boolean is not installed.  Be loud about it.
            raise ThreeMFError(
                "boolean union requires a trimesh boolean back-end "
                "(manifold3d or similar).  Install one with "
                "`pip install manifold3d` or use add_modifier() instead."
            )

        xml = self._entries["3D/3dmodel.model"]
        xml = _splice_object_mesh(xml, object_id, _mesh_xml(merged))
        return self._save_with(xml, save_as)

    # ---- save ----------------------------------------------------------

    def _save_with(self, modified_xml: bytes, save_as) -> "Bundle":
        """Write a new ZIP whose only difference from the source is the
        modified 3dmodel.model (and any 3D/Objects/Object_N.model that
        referenced its mesh - rare in Bambu, omitted for clarity)."""
        out = Path(save_as) if save_as is not None else \
            self.source_path.with_name(self.source_path.stem + "_mod.3mf")
        if out == self.source_path:
            raise ThreeMFError(
                "save_as must differ from source path (would otherwise truncate input)"
            )
        with zipfile.ZipFile(out, "w", zipfile.ZIP_DEFLATED) as z:
            for name, data in self._entries.items():
                payload = modified_xml if name == "3D/3dmodel.model" else data
                zi = zipfile.ZipInfo(filename=name)
                zi.compress_type = zipfile.ZIP_DEFLATED
                zi.external_attr = 0o644 << 16
                z.writestr(zi, payload)
        return Bundle(source_path=out,
                      _entries=self._with_replacement(out))

    def _with_replacement(self, path: Path) -> dict[str, bytes]:
        """Re-read a freshly-saved 3MF so further edits are valid."""
        entries: dict[str, bytes] = {}
        with zipfile.ZipFile(path, "r") as z:
            for info in z.infolist():
                entries[info.filename] = z.read(info.filename)
        return entries

    # ---- alt: bare-bones write ----------------------------------------

    def save(self, save_as=None) -> Path:
        """Write the current state of the bundle to disk (no edits)."""
        out = Path(save_as) if save_as is not None else \
            self.source_path.with_name(self.source_path.stem + "_out.3mf")
        with zipfile.ZipFile(out, "w", zipfile.ZIP_DEFLATED) as z:
            for name, data in self._entries.items():
                zi = zipfile.ZipInfo(filename=name)
                zi.compress_type = zipfile.ZIP_DEFLATED
                z.writestr(zi, data)
        return out


# ------------------------------------------------------------------- loader


def load(path: str | Path) -> Bundle:
    """Open a .3mf into a Bundle.  All entries are held in memory."""
    p = Path(path)
    if not p.exists():
        raise ThreeMFError(f"file not found: {p}")
    entries: dict[str, bytes] = {}
    with zipfile.ZipFile(p, "r") as z:
        for info in z.infolist():
            entries[info.filename] = z.read(info.filename)
    if "3D/3dmodel.model" not in entries:
        raise ThreeMFError(
            f"{p} is not a 3MF archive (no 3D/3dmodel.model inside)"
        )
    return Bundle(source_path=p, _entries=entries)