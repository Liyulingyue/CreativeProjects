---
name: cadquery-3d-print-designer
description: Parameterized 3D printing design pipeline using CadQuery (CSG) + PyVista (preview) + Trimesh (3MF round-trip). Generates watertight STL, renders multi-view PNG/SVG previews, AND round-trips Bambu Studio / OrcaSlicer .3mf files without losing slicer-specific metadata (color painting, print profiles, plate layout) via a dual-engine unpack-modify-repack pipeline. Use when designing 3D printable brackets, enclosures, adapters, fan mounts, NUC mounts, parametric hardware, or when you need to surgically replace / append geometry inside an existing Bambu .3mf project.
---

# CadQuery 3D Print Designer

A specialized workflow for generating parametric 3D models for additive manufacturing (3D printing) **and** for modifying existing Bambu Studio / OrcaSlicer 3MF projects in place. Combines three engines, each used for the one thing it does best:

| Engine | Job |
| --- | --- |
| **CadQuery** | Parameterised CSG geometry generation (screw bosses, grilles, ribs, enclosures) |
| **PyVista** | Off-screen multi-view preview rendering (PNG) of the resulting solids |
| **Trimesh + zipfile** | 3MF round-trip: read the slicer ZIP, splice the new mesh in, repack **without trampling Bambu metadata** |

## When to Trigger

Activate this skill when the user requests:
- Parametric 3D modeling for 3D printing: brackets, enclosures, fan mounts, NUC mounts, adapters.
- Modifying an existing Bambu / Orca `.3mf`: add a vent grille to a part, append a modifier mesh, union a feature into a print job, recover a part's bounding box for further design.
- Anything mentioning CadQuery, Trimesh, 3MF, STL generation, parametric CAD design, or Bambu/Orca round-trip.

## Architecture: Three Engines, Not One

There are two distinct pipelines. Use the one that fits the task; do not mix them.

### Pipeline A — From Scratch (CadQuery → STL → Preview)

```
parameters  ->  CadQuery CSG  ->  STL file  ->  PyVista multi-view PNG
                                    ^
                                    | feedback loop until visually correct
```

Use when the user has no existing 3MF, or only has dimensions.

### Pipeline B — Round-Trip a Bambu / Orca .3mf (unpack → modify → repack)

```
input.3mf  --[trimesh read]-->  scene (geometry + bbox)
            --[zipfile read]--> ALL metadata (paint, profile, plate, ...)
                                   |
                                   v
                         CadQuery CSG geometry
                                   |
                                   v
                  [byte-level XML splice into 3dmodel.model]
                                   |
                                   v
output.3mf  <-- repack ZIP, copying every original byte except the spliced files
```

Use when the user already has a Bambu / Orca project with color painting, a tuned print profile, or a plate layout they want to keep.

**Critical rule**: never re-emit the entire 3MF from scratch (`trimesh.Scene.export("out.3mf")`). That gives a technically valid 3MF but tramples every slicer-specific XML file inside the ZIP. Always unpack → modify → repack.

## Recommended Python Execution Blueprint

### Prerequisites: Virtual Environment Setup

```bash
python -m venv .venv
# Windows (PowerShell):
.venv\Scripts\Activate.ps1
# Linux / macOS:
source .venv/bin/activate

pip install --upgrade pip
pip install cadquery pyvista trimesh
# Optional, for boolean union inside 3MF round-trip:
pip install manifold3d
```

Verify:
```bash
python -c "import cadquery as cq, pyvista as pv, trimesh; print('cq', cq.__version__, 'pv', pv.__version__, 'trimesh', trimesh.__version__)"
```

Linux also needs `xvfb` for PyVista off-screen rendering.

---

## Pipeline A — From Scratch

### Step 1: CadQuery Modeling & STL Export

```python
import cadquery as cq

length, width, height = 100.0, 100.0, 20.0
wall_thick = 3.0
hole_diam = 80.0
screw_pitch = 71.5
screw_diam = 4.3  # M4 over-hole clearance

model = (
    cq.Workplane("XY")
    .box(length, width, height)
    .edges("|Z").fillet(5.0)
    .faces(">Z").workplane()
    .hole(hole_diam)
    .rect(screw_pitch, screw_pitch, forConstruction=True)
    .vertices()
    .hole(screw_diam)
)

cq.exporters.export(model, "output_model.stl")
```

For a fully watertight perforated plate, prefer the built-in generators:

```python
from generators import grid_panel, ribbed_plate

# A flat 266x156 chassis panel with a single-direction vent grille
panel = grid_panel(length=266, width=156, thickness=3, rib=4, gap=5, direction="u")
panel.save("chassis_wall.stl")

# A cross-lattice grille that stays one solid because ribs sit on a base
ribs = ribbed_plate(length=266, width=156, base=3, rib_w=4, gap=5, rib_h=6, direction="both")
ribs.save("cross_grille.stl")
```

### Step 2: PyVista Off-Screen Multi-View Rendering

```python
import pyvista as pv

def render_preview(stl_path="output_model.stl", output_img="preview_iso.png"):
    mesh = pv.read(stl_path)
    plotter = pv.Plotter(off_screen=True)
    plotter.add_mesh(mesh, color="whitesmoke", show_edges=True, edge_color="gray", specular=0.5)
    plotter.add_bounding_box(color="black", line_width=1)
    plotter.show_bounds(grid='back', location='outer', ticks='both',
                        xlabel='X (mm)', ylabel='Y (mm)', zlabel='Z (mm)')
    plotter.view_isometric()
    plotter.screenshot(output_img)
    plotter.close()

render_preview()
```

### Step 3: Zero-Dependency Fallback (CadQuery Native SVG)

```python
cq.exporters.export(
    model, "preview_fallback.svg",
    opt={"width": 800, "height": 600, "showAxes": True,
         "projectionDir": (1, 1, 1), "strokeWidth": 1.2},
)
```

---

## Pipeline B — Round-Trip a Bambu / Orca .3mf

### Step 1: Open the Bundle

```python
from generators import three_mf

bundle = three_mf.load("input_bambu.3mf")
print("parts:", [(p.object_id, p.name, p.size) for p in bundle.list_parts()])
```

`load()` reads the entire 3MF ZIP into memory as a `Bundle`. Every original byte — paint data, print profile, plate layout, thumbnail, Bambu namespaces — is preserved verbatim.

### Step 2: Get the Part's Bounding Box

```python
info = bundle.get_part(object_id=1)            # or name=... or omit for first leaf
length, width, depth = info.size              # (dx, dy, dz) in mm
# or aggregate all parts:
agg_bbox = bundle.bbox()                       # np.ndarray shape (2, 3)
```

### Step 3: Generate Geometry with CadQuery

Hand the dimensions to the existing generators:

```python
from generators import grid_panel

vent = grid_panel(length=length, width=width, thickness=3,
                  rib=4, gap=5, direction="u")
```

Or write a fully custom CadQuery solid — anything that ends up as a watertight `cq.Workplane` works.

### Step 4: Splice into the Original 3MF

Pick one of three operations. **Choose by what you want the user to keep**, not by what is most automatic:

#### Option A — `replace_mesh`: surgical in-place replacement
```python
bundle.replace_mesh(object_id=1, shape=vent, save_as="output_bambu.3mf")
```
The selected part's geometry is replaced. Every other part, all paint, all profile data, all plate layout — untouched. Use when the user has not yet painted the part or does not care about losing paint on this specific part.

#### Option B — `add_modifier`: append as a Modifier mesh (RECOMMENDED for painted parts)
```python
bundle.add_modifier(
    shape=vent,
    save_as="with_grille_modifier.3mf",
    translate=(0.0, 0.0, 0.0),         # move the modifier to align with the part
)
```
Emits `type="other"` so Bambu Studio loads it as a candidate Modifier. The user right-clicks the part in the part list and chooses **Type -> Modifier** (or sets Top/Bottom Shell = 0 + infill pattern = 20% Hexagonal in Bambu). This is the cleanest way to add a grille to a *painted* part because the painted mesh is never touched.

#### Option C — `union_replace`: boolean merge
```python
bundle.union_replace(object_id=1, shape=vent, save_as="merged.3mf")
```
Rebuilds vertices from scratch. **Drops per-vertex color painting on this part.** Only use when you actually want the geometry merged into one body and there is no paint to lose.

### Step 5: Done

The output `.3mf` opens in Bambu Studio with the modified geometry and all original metadata intact.

---

## Bambu / Orca 3MF Pitfalls & How This Skill Avoids Them

1. **Color painting is per-vertex.** Any boolean / re-tessellation drops it. **Use `add_modifier`** for painted parts.
2. **Print profiles live in `Metadata/project_settings.config`.** Bambu reads them on open; if missing, the print goes back to defaults. We preserve the file byte-for-byte.
3. **Plate layout lives in `Metadata/slice_info.config` / `Metadata/plate_1.json`.** Same story.
4. **Bambu uses extra namespaces** (`p:`, `bambu:`) in `3dmodel.model`. Generic XML writers often strip or rewrite them. We splice the mesh block byte-for-byte and leave every namespace declaration, attribute order, and whitespace outside the mesh untouched.
5. **Some 3MFs split meshes across `3D/Objects/Object_N.model`.** The current implementation only handles the common case where every mesh lives inside `3D/3dmodel.model`. If the user reports a "no <mesh> in object N" error, ask them to flatten their 3MF in Bambu Studio first (**File → Export → 3MF**, do not "Save").
6. **Modifiers are set in two places**: the XML `type="other"` (we emit this) and the user's right-click in Bambu Studio (we cannot do this). After importing, the user must right-click → **Type → Modifier**.
7. **Boolean union requires a back-end**: trimesh ships with `manifold3d` support but does not install it by default. If `union_replace` raises, run `pip install manifold3d` or fall back to `add_modifier`.

## Conventions for Generator Output

Whether generated from scratch or spliced into a 3MF, every part must be:
- A single watertight solid (one shell, one set of faces with consistent outward normals).
- Manifold: every edge shared by exactly two faces.
- Coordinate-convention: XY is the flat face, Z is the print direction.

`generators.core.verify()` checks all three. `generators.core.analytic_volume()` checks the volume against a hand-derived formula for the lattice parts, so a broken slot that lands on a wall face fails loudly instead of silently shipping a non-perforated plate. Run the tests:

```bash
python -m generators.selftest          # CSG generators (CadQuery required)
python -m generators.three_mf_selftest # 3MF round-trip (CadQuery required for shape bridges; trimesh-only paths also covered)
```

## Dependencies

- Python >= 3.9
- `cadquery >= 2.4` (CSG)
- `pyvista >= 0.40` (preview; optional — falls back to CadQuery SVG)
- `trimesh >= 4.0` (3MF read + mesh bridge)
- `numpy` (transitive via trimesh)
- `manifold3d` (optional, only needed for `union_replace`)
- Linux: `xvfb` for off-screen PyVista

`generators/three_mf.py` keeps `cadquery` as a *lazy* import so reading/inspecting a Bambu 3MF works with `trimesh` alone — useful for QA scripts and for CI smoke tests that do not need the full OCCT stack.