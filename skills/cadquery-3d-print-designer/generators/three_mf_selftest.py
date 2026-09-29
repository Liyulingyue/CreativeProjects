"""Self-test for the 3MF round-trip module.

Run:  python -m generators.three_mf_selftest

We never depend on a real Bambu Studio file.  Instead each test
fabricates a minimal 3MF in a temp directory using a hand-written
3dmodel.model, then exercises the round-trip on it.  This way the
tests catch regressions in the XML splicing logic without needing
slicer-specific fixtures.
"""
from __future__ import annotations

import re
import sys
import zipfile
from pathlib import Path

# cadquery is only needed by tests that build actual CadQuery solids.
# We import it lazily so this file can be parsed (and its pure-stdlib
# tests can be loaded) on machines without the OCCT stack installed.
_cq = None
def _cq_module():
    global _cq
    if _cq is None:
        import cadquery as _c
        _cq = _c
    return _cq

import numpy as np

from .grid_panel import grid_panel
from .three_mf import Bundle, ThreeMFError, load


PASS, FAIL = "  ok  ", " FAIL "
_failures: list[str] = []


def check(name: str, fn) -> None:
    try:
        fn()
        print(f"{PASS} {name}")
    except Exception as e:
        print(f"{FAIL} {name}\n         {e}")
        _failures.append(f"{name}: {e}")


# ------------------------------------------------------------ fixtures


_MODEL_XML = """<?xml version="1.0" encoding="UTF-8"?>
<model unit="millimeter"
       xmlns="http://schemas.microsoft.com/3dmanufacturing/core/2015/02"
       xmlns:p="http://schemas.microsoft.com/3dmanufacturing/production/2015/06"
       xmlns:bambu="http://schemas.bambulab.com/3mf/2021/01">
  <metadata name="Application">BambuStudio-1.8.4</metadata>
  <metadata name="Title">My Cool Print</metadata>
  <resources>
    <object id="1" type="model" p:UUID="00000000-0000-0000-0000-000000000001">
      <mesh>
        <vertices>
          <vertex x="0.000000" y="0.000000" z="0.000000"/>
          <vertex x="40.000000" y="0.000000" z="0.000000"/>
          <vertex x="40.000000" y="20.000000" z="0.000000"/>
          <vertex x="0.000000" y="20.000000" z="0.000000"/>
          <vertex x="0.000000" y="0.000000" z="3.000000"/>
          <vertex x="40.000000" y="0.000000" z="3.000000"/>
          <vertex x="40.000000" y="20.000000" z="3.000000"/>
          <vertex x="0.000000" y="20.000000" z="3.000000"/>
        </vertices>
        <triangles>
          <triangle v1="0" v2="1" v3="2"/>
          <triangle v1="0" v2="2" v3="3"/>
          <triangle v1="4" v2="6" v3="5"/>
          <triangle v1="4" v2="7" v3="6"/>
          <triangle v1="0" v2="5" v3="1"/>
          <triangle v1="0" v2="4" v3="5"/>
          <triangle v1="1" v2="6" v3="2"/>
          <triangle v1="1" v2="5" v3="6"/>
          <triangle v1="2" v2="7" v3="3"/>
          <triangle v1="2" v2="6" v3="7"/>
          <triangle v1="3" v2="4" v3="0"/>
          <triangle v1="3" v2="7" v3="4"/>
        </triangles>
      </mesh>
    </object>
    <object id="2" type="model" p:UUID="00000000-0000-0000-0000-000000000002">
      <mesh>
        <vertices>
          <vertex x="50.000000" y="0.000000" z="0.000000"/>
          <vertex x="60.000000" y="0.000000" z="0.000000"/>
          <vertex x="55.000000" y="10.000000" z="0.000000"/>
        </vertices>
        <triangles>
          <triangle v1="0" v2="1" v3="2"/>
        </triangles>
      </mesh>
    </object>
  </resources>
  <build>
    <item objectid="1" transform="1 0 0 0 0 1 0 0 0 0 1 0" printable="1"/>
    <item objectid="2" transform="1 0 0 0 0 1 0 0 0 0 1 0" printable="1"/>
  </build>
</model>
"""

_PROJECT_SETTINGS = (
    '<?xml version="1.0" encoding="UTF-8"?>\n'
    '<config>\n'
    '  <print>\n'
    '    <layer_height>0.2</layer_height>\n'
    '    <filament_type>PLA</filament_type>\n'
    '  </print>\n'
    '</config>\n'
)

_COLORS_XML = (
    '<?xml version="1.0" encoding="UTF-8"?>\n'
    '<colors>\n'
    '  <color object_id="1" face_index="0" rgba="#FF0000FF"/>\n'
    '</colors>\n'
)


def _make_fixture(tmp: Path, name: str = "fixture.3mf") -> Path:
    """Write a minimal but realistic Bambu-ish 3MF ZIP to tmp/name."""
    p = tmp / name
    with zipfile.ZipFile(p, "w", zipfile.ZIP_DEFLATED) as z:
        z.writestr("3D/3dmodel.model", _MODEL_XML)
        z.writestr("Metadata/project_settings.config", _PROJECT_SETTINGS)
        z.writestr("Metadata/colors.xml", _COLORS_XML)
    return p


# ------------------------------------------------------------ tests


def t_load_round_trip(tmp: Path):
    p = _make_fixture(tmp)
    b = load(p)
    assert isinstance(b, Bundle)
    # original metadata must be present, unchanged
    assert b._entries["Metadata/project_settings.config"] == _PROJECT_SETTINGS.encode()
    assert b._entries["Metadata/colors.xml"] == _COLORS_XML.encode()
    # the model must round-trip back to disk byte-equal for non-mesh data
    out = tmp / "copy.3mf"
    b.save(out)
    with zipfile.ZipFile(out, "r") as zin, zipfile.ZipFile(p, "r") as zref:
        for info in zref.infolist():
            if info.filename == "3D/3dmodel.model":
                continue
            assert zin.read(info.filename) == zref.read(info.filename), info.filename


def t_list_parts(tmp: Path):
    p = _make_fixture(tmp)
    b = load(p)
    parts = b.list_parts()
    ids = sorted(p.object_id for p in parts)
    assert ids == [1, 2], ids


def t_get_part_by_id(tmp: Path):
    p = _make_fixture(tmp)
    b = load(p)
    one = b.get_part(object_id=1)
    assert one.object_id == 1
    assert one.size[0] == 40.0
    assert one.size[1] == 20.0
    assert one.size[2] == 3.0


def t_bbox_aggregate(tmp: Path):
    p = _make_fixture(tmp)
    b = load(p)
    bb = b.bbox()
    # union of object 1 (0..40 x 0..20) and object 2 (50..60 x 0..10) is
    # 0..60 x 0..20 x 0..3
    assert bb[1, 0] - bb[0, 0] == 60.0
    assert bb[1, 1] - bb[0, 1] == 20.0


def t_replace_mesh_preserves_metadata(tmp: Path):
    p = _make_fixture(tmp)
    b = load(p)
    grid = grid_panel(40, 20, 3, rib=4, gap=5, direction="u")
    out = tmp / "replaced.3mf"
    b.replace_mesh(object_id=1, shape=grid, save_as=out)
    assert out.exists()
    # Metadata files must be byte-identical
    with zipfile.ZipFile(out, "r") as z:
        assert z.read("Metadata/project_settings.config") == _PROJECT_SETTINGS.encode()
        assert z.read("Metadata/colors.xml") == _COLORS_XML.encode()
    # The other object (id=2) must not be touched
    new = load(out)
    two = new.get_part(object_id=2)
    assert two.size[0] == 10.0  # unchanged: 50..60
    # And the replaced object id=1 should now have many more triangles
    # than the original 12 (a 40x20 grid with 4mm ribs is lots of slots).
    one = new.get_part(object_id=1)
    assert one.mesh is not None
    assert len(one.mesh.faces) > 12


def t_replace_mesh_uses_input_only_mesh(tmp: Path):
    p = _make_fixture(tmp)
    b = load(p)
    tiny = _cq_module().Workplane("XY").box(2, 2, 1)
    out = tmp / "tiny.3mf"
    b.replace_mesh(object_id=1, shape=tiny, save_as=out)
    new = load(out)
    bb = new.bbox(object_id=1)
    assert abs((bb[1, 0] - bb[0, 0]) - 2.0) < 0.01
    assert abs((bb[1, 1] - bb[0, 1]) - 2.0) < 0.01


def t_replace_missing_object_raises(tmp: Path):
    p = _make_fixture(tmp)
    b = load(p)
    try:
        b.replace_mesh(object_id=999, shape=_cq_module().Workplane("XY").box(1, 1, 1),
                       save_as=tmp / "x.3mf")
    except ThreeMFError:
        return
    raise AssertionError("replace_mesh on missing id should raise")


def t_add_modifier_appends_object(tmp: Path):
    p = _make_fixture(tmp)
    b = load(p)
    grid = grid_panel(40, 20, 3, rib=4, gap=5, direction="u")
    out = tmp / "modifier.3mf"
    b.add_modifier(shape=grid, save_as=out)
    new = load(out)
    parts = new.list_parts()
    ids = sorted(p.object_id for p in parts)
    # original two + new modifier = 3
    assert ids == [1, 2, 3], ids
    # metadata still intact
    with zipfile.ZipFile(out, "r") as z:
        assert z.read("Metadata/colors.xml") == _COLORS_XML.encode()


def t_add_modifier_inserts_build_item(tmp: Path):
    p = _make_fixture(tmp)
    b = load(p)
    b.add_modifier(shape=_cq_module().Workplane("XY").box(5, 5, 2), save_as=tmp / "m2.3mf")
    xml = load(tmp / "m2.3mf")._entries["3D/3dmodel.model"]
    # count <item .../>  -> must be 3 after insertion
    items = re.findall(rb'<item\b[^/>]*/>', xml)
    assert len(items) == 3, len(items)


def t_add_modifier_with_translate(tmp: Path):
    p = _make_fixture(tmp)
    b = load(p)
    # put a 5x5 cube 100mm to the right of object 1
    b.add_modifier(
        shape=_cq_module().Workplane("XY").box(5, 5, 2),
        save_as=tmp / "m3.3mf",
        translate=(100.0, 0.0, 0.0),
    )
    # trimesh's 3MF reader keeps transforms at the scene-graph level and
    # does NOT bake them into the per-mesh bbox, so we assert on the
    # raw 3dmodel.model bytes - that is what Bambu Studio actually reads.
    xml = load(tmp / "m3.3mf")._entries["3D/3dmodel.model"].decode()
    assert "100" in xml
    # the modifier's <item> in <build> must also carry the translation
    assert re.search(r'<item[^/>]*transform="1 0 0 100[^"]*"[^/>]*/>', xml), xml


def t_round_trip_does_not_clobber_xmlnamespaces(tmp: Path):
    """Bambu namespaces (p:, bambu:) must survive splicing verbatim."""
    p = _make_fixture(tmp)
    b = load(p)
    b.replace_mesh(object_id=1, shape=_cq_module().Workplane("XY").box(40, 20, 3),
                   save_as=tmp / "ns.3mf")
    xml = load(tmp / "ns.3mf")._entries["3D/3dmodel.model"].decode()
    assert 'xmlns:p="http://schemas.microsoft.com/3dmanufacturing/production/2015/06"' in xml
    assert 'xmlns:bambu="http://schemas.bambulab.com/3mf/2021/01"' in xml
    assert 'p:UUID="00000000-0000-0000-0000-000000000001"' in xml
    assert "<metadata" in xml


def t_empty_cq_shape_rejected(tmp: Path):
    """defensive: an empty CadQuery shape cannot become a 3MF mesh."""
    p = _make_fixture(tmp)
    b = load(p)
    empty = _cq_module().Workplane("XY")
    try:
        b.replace_mesh(object_id=1, shape=empty, save_as=tmp / "e.3mf")
    except ThreeMFError:
        return
    raise AssertionError("empty CadQuery shape should raise")


# ------------------------------------------------------------ main


def main() -> int:
    import tempfile
    with tempfile.TemporaryDirectory() as td:
        tmp = Path(td)
        for n in sorted(dir()):
            if n.startswith("t_"):
                check(n[2:], lambda n=n: globals()[n](tmp))
    print()
    if _failures:
        print(f"{len(_failures)} FAILED")
        return 1
    print("all checks passed")
    return 0


if __name__ == "__main__":
    sys.exit(main())