"""3MF round-trip demo.  Run:  python -m generators.three_mf_demo

Builds a synthetic Bambu-like 3MF in a temp directory, sizes a vent
grille from the part's bounding box, and writes three variants:

    sample_in.3mf          original (synthetic)
    sample_replaced.3mf    mesh of part 1 swapped with the grille
    sample_modifier.3mf    grille appended as a modifier
    sample_merged.3mf      boolean union of original + grille
                           (requires manifold3d; skipped otherwise)

Prints which 3MF files were written and what their bbox / part counts
are, so you can eyeball that metadata survived the round-trip.
"""
from __future__ import annotations

import sys
import tempfile
import zipfile
from pathlib import Path

import numpy as np

from .grid_panel import grid_panel
from .three_mf import load


# Same minimal-but-realistic Bambu fixture used by the self-test.
_MODEL_XML = """<?xml version="1.0" encoding="UTF-8"?>
<model unit="millimeter"
       xmlns="http://schemas.microsoft.com/3dmanufacturing/core/2015/02"
       xmlns:p="http://schemas.microsoft.com/3dmanufacturing/production/2015/06"
       xmlns:bambu="http://schemas.bambulab.com/3mf/2021/01">
  <metadata name="Application">BambuStudio-1.8.4</metadata>
  <metadata name="Title">Demo Part</metadata>
  <resources>
    <object id="1" type="model" p:UUID="00000000-0000-0000-0000-000000000001">
      <mesh>
        <vertices>
          <vertex x="0" y="0" z="0"/><vertex x="40" y="0" z="0"/>
          <vertex x="40" y="20" z="0"/><vertex x="0" y="20" z="0"/>
          <vertex x="0" y="0" z="3"/><vertex x="40" y="0" z="3"/>
          <vertex x="40" y="20" z="3"/><vertex x="0" y="20" z="3"/>
        </vertices>
        <triangles>
          <triangle v1="0" v2="1" v3="2"/><triangle v1="0" v2="2" v3="3"/>
          <triangle v1="4" v2="6" v3="5"/><triangle v1="4" v2="7" v3="6"/>
          <triangle v1="0" v2="5" v3="1"/><triangle v1="0" v2="4" v3="5"/>
          <triangle v1="1" v2="6" v3="2"/><triangle v1="1" v2="5" v3="6"/>
          <triangle v1="2" v2="7" v3="3"/><triangle v1="2" v2="6" v3="7"/>
          <triangle v1="3" v2="4" v3="0"/><triangle v1="3" v2="7" v3="4"/>
        </triangles>
      </mesh>
    </object>
  </resources>
  <build>
    <item objectid="1" transform="1 0 0 0 0 1 0 0 0 0 1 0" printable="1"/>
  </build>
</model>
"""

_PROJECT = (
    '<?xml version="1.0" encoding="UTF-8"?>\n'
    '<config>\n'
    '  <print>\n'
    '    <layer_height>0.2</layer_height>\n'
    '    <filament_type>PLA</filament_type>\n'
    '  </print>\n'
    '</config>\n'
)

_COLORS = (
    '<?xml version="1.0" encoding="UTF-8"?>\n'
    '<colors>\n'
    '  <color object_id="1" face_index="0" rgba="#FF0000FF"/>\n'
    '</colors>\n'
)


def _make_sample(out: Path) -> Path:
    p = out / "sample_in.3mf"
    with zipfile.ZipFile(p, "w", zipfile.ZIP_DEFLATED) as z:
        z.writestr("3D/3dmodel.model", _MODEL_XML)
        z.writestr("Metadata/project_settings.config", _PROJECT)
        z.writestr("Metadata/colors.xml", _COLORS)
    return p


def main(out_dir: str | Path | None = None) -> int:
    out = Path(out_dir) if out_dir else Path(tempfile.mkdtemp(prefix="threemf_demo_"))
    out.mkdir(parents=True, exist_ok=True)

    src = _make_sample(out)
    bundle = load(src)

    info = bundle.get_part(object_id=1)
    print(f"loaded  {src.name}")
    print(f"  part id={info.object_id}  size={info.size}  "
          f"bbox_min={info.bbox[0].tolist()}  bbox_max={info.bbox[1].tolist()}")

    # Build a grille sized to the part
    grille = grid_panel(
        length=info.size[0], width=info.size[1],
        thickness=info.size[2], rib=4, gap=5, direction="u",
    )
    print(f"  grille: {grille}")

    replaced = out / "sample_replaced.3mf"
    bundle.replace_mesh(object_id=1, shape=grille, save_as=replaced)
    print(f"wrote   {replaced.name}  (replace_mesh)")

    modifier = out / "sample_modifier.3mf"
    bundle.add_modifier(shape=grille, save_as=modifier)
    print(f"wrote   {modifier.name}  (add_modifier)")

    merged = out / "sample_merged.3mf"
    try:
        bundle.union_replace(object_id=1, shape=grille, save_as=merged)
        print(f"wrote   {merged.name}  (union_replace)")
    except Exception as e:
        print(f"skipped sample_merged.3mf  (union_replace needs manifold3d: {e})")

    print(f"\noutputs in: {out}")
    for f in sorted(out.iterdir()):
        b = load(f)
        # Verify the colour/paint metadata survived every variant
        paint = b._entries.get("Metadata/colors.xml")
        paint_ok = paint == _COLORS.encode()
        print(f"  {f.name}  parts={len(b.list_parts())}  paint_preserved={paint_ok}")
    return 0


if __name__ == "__main__":
    out = sys.argv[1] if len(sys.argv) > 1 else None
    sys.exit(main(out))