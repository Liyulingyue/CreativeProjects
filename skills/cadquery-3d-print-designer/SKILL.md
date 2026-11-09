---
name: cadquery-3d-print-designer
description: Parameterized 3D printing design pipeline using CadQuery (CSG) + PyVista (preview) + Trimesh (3MF round-trip). Generates watertight STL, renders multi-view PNG/SVG previews, AND round-trips Bambu Studio / OrcaSlicer .3mf files without losing slicer-specific metadata (color painting, print profiles, plate layout) via a dual-engine unpack-modify-repack pipeline. Ships parametric generators for vent grilles, ribbed plates, hanger-hook caps, table-tennis blades and paddles, and the Meituan LongCat logo (flat / split-colour / bulging-3D-head builds). Use when designing 3D printable brackets, enclosures, adapters, fan mounts, NUC mounts, parametric hardware, or when you need to surgically replace / append geometry inside an existing Bambu .3mf project.
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

**If PyPI is slow, use a mirror** — the `cadquery-ocp` wheel alone is ~68 MB and can stall for many minutes on the default index:

```bash
pip install -i https://pypi.tuna.tsinghua.edu.cn/simple cadquery pyvista trimesh manifold3d
```

### Headless rendering (Linux)

PyVista renders through VTK, which needs a GL context. On a headless box there are two ways to get one:

1. **EGL / OSMesa (preferred, no root needed).** VTK falls back to it automatically when `DISPLAY` is unset, provided the Mesa libraries are present. On Ubuntu these are `libegl1`, `libegl-mesa0` and `libgl1-mesa-dri`; verify with `ldconfig -p | grep -E 'libEGL|libGL'`. Nothing else to install.
2. **Xvfb (fallback).** If VTK complains about a missing GL context, install it: `sudo apt install xvfb`, then call `pv.start_xvfb()` before rendering.

Both give software rasterisation — fine for CAD previews, no GPU required. Expect one harmless `bad X server connection. DISPLAY=` warning on stderr; the PNG is still written correctly.

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
from examples.grid_panel import grid_panel
from examples.ribbed_plate import ribbed_plate

# A flat 266x156 chassis panel with a single-direction vent grille
panel = grid_panel(length=266, width=156, thickness=3, rib=4, gap=5, direction="u")
panel.save("chassis_wall.stl")

# A cross-lattice grille that stays one solid because ribs sit on a base
ribs = ribbed_plate(length=266, width=156, base=3, rib_w=4, gap=5, rib_h=6, direction="both")
ribs.save("cross_grille.stl")
```

### Step 1b: Worked Examples

`examples/` holds six complete, working generators. They exist as **reference
implementations** — each one shows a different modelling technique — not as
products. Read the module's own docstring for its parameters; the table below
only says which technique each one demonstrates.

| example | what it is | technique it demonstrates |
| --- | --- | --- |
| `examples/grid_panel.py` | perforated chassis panel | lattice layout + **analytic volume** cross-check |
| `examples/ribbed_plate.py` | cross-lattice grille | a grid that must stay one solid |
| `examples/hanger_hook.py` | sleeve for a clothes hanger hook tip | a **swept** cavity that follows a curve |
| `examples/blade.py` | one-piece table-tennis blade | a width profile → **PCHIP** spline, mitred inward offset for the bevelled rim |
| `examples/paddle.py` | quick-release table-tennis paddle | a **multi-part assembly** with a dovetail joint and two screws |
| `examples/longcat_logo.py` | the Meituan LongCat logo, three ways | SVG path parsing, colour splits, a domed 3D head |

```python
from examples.hanger_hook import hanger_hook_cap, hanger_hook

cap = hanger_hook_cap(wire_dia=4.0, hook_r=15.0, cap_len=12.0)
cap.save("hook_cap.stl")
print(cap)
# hanger_hook_cap  wire 4.0  hook R 15.0  cap 12.0 long
#   cavity r 2.30  outer r 4.10  wall 1.8
#   covers 45.8 deg of the bend, tip at -70.0 deg
#   mass 0.69 g

hook = hanger_hook()          # reference hook, to check the fit visually
```

The hook cap's cavity is a swept tube around the wire centreline and its wall is
a constant offset of that, so the fit is geometric rather than eyeballed.

### Step 1c: Meituan LongCat Logo (flat / split-colour / 3D head)

`examples/longcat_logo.py` holds the Meituan LongCat logo — an M-shaped outline
with a V notch between the ears, a rounded cat face, and two slanted eyes —
parsed from the real SVG path data, so the proportions are exact. Three builds
share the same source outlines:

```python
from examples.longcat_logo import longcat_logo, longcat_logo_split, longcat_logo_3d

# one flat plate; face and eyes are recessed pockets (filament swap / paint)
longcat_logo().save("logo.stl")

# three flat pieces that slot together, for printing in three colours
parts = longcat_logo_split()
for name, part in parts.items():
    part.save(f"{name}.stl")

# the same three pieces, but the head is a bulging solid that rises out of
# the shell; eyes sit flush with the head surface
parts = longcat_logo_3d()
parts["cat_head"].save("cat_head.stl")
```

| build | function | use when |
| --- | --- | --- |
| flat | `longcat_logo()` | one-colour print, colour zones come from a filament swap or painting |
| split | `longcat_logo_split()` | multi-colour print from separate STLs / MMU |
| 3D | `longcat_logo_3d()` | the head should stand out as a rounded solid |

| parameter (`HeadSpec`) | default | meaning |
| --- | --- | --- |
| `thickness` | 5.0 | flat M shell thickness (Z) — **also the head's rim width** |
| `front_h` / `back_h` | 6.0 / 5.0 | how far the head bulges above / below the shell |
| `eye_depth` | 1.5 | how deep the eye pockets are cut into the head |

The 3D head's side profile is **arc – straight – arc**: the straight rim at
the edge is exactly `thickness` wide (the same width as the M shell), and the
two arcs bulge freely. The rim is the face outline itself, so it meets the
shell with no gap and no overhang.

`save_split(parts, outdir)` writes one STL per component; `save_split_3mf`
writes them as a 3-object 3MF so the slicer's colour picker sees each part.

Two implementation notes worth keeping if you extend this:

* **Do not build the dome from a scaled sphere.** OCCT's sphere tessellation
  leaves a 4-edge crack at the pole, so the exported STL is not watertight.
  `_dome` lofts the outline to a point instead; `_ellipsoid` (a loft of
  ellipse wires) is the watertight alternative when a smooth cap is wanted.
* **The rim must be the unscaled outline.** Scaling the outline about its
  centroid and lofting *that* makes the rim wider than the shell opening, so
  the head collides with the M. The rim stays at `thickness`; only the bulge
  grows.

### Step 2: PyVista Off-Screen Multi-View Rendering

Use the shared renderer. It accepts a generator result, a `cq.Workplane` or a `cq.Shape` — the `GridPanel` / `RibbedPlate` dataclasses are unwrapped automatically, so you never have to pass `p.shape` by hand:

```python
from examples.grid_panel import grid_panel
from pipeline.render import render

panel = grid_panel(length=266, width=156, thickness=3, rib=4, gap=5, direction="u")
render(panel, "panel", views=("iso", "front", "top"), outdir="out")
# -> out/panel.stl, out/panel_iso.png, out/panel_front.png, out/panel_top.png
```

Useful flags:

| flag | effect |
| --- | --- |
| `edges=30` | draw only feature edges sharper than 30° — **the STL triangulation is dropped** |
| `edges=True` | draw every edge, including the STL's triangle boundaries |
| `bounds=True` | add the bounding box and X/Y/Z axes with mm labels |
| `extra=<shape>` | overlay a second part translucently as a reference |

**Why `edges` matters.** On a tessellated STL, PyVista's `show_edges=True` draws the *triangle* boundaries, not the part's feature edges. A flat panel then renders as a grid of little squares instead of a row of slots — the geometry is fine, the picture is lying. `edges=30` keeps the real corners and drops the triangulation (whose dihedral angle is 0).

> **`add_mesh(feature_angle=...)` does NOT do this.** In PyVista 0.49 `feature_angle` only controls how normals are split for smooth shading; the `show_edges` overlay always draws the triangle boundaries, whatever you pass. To get real feature edges you have to extract them yourself and add them as a second actor — which is what `render(edges=30)` does.

If you need a hand-rolled plot instead of the shared renderer:

```python
import pyvista as pv

mesh = pv.read("output_model.stl")
edges = mesh.extract_feature_edges(
    feature_angle=30, feature_edges=True, boundary_edges=True,
    non_manifold_edges=False, manifold_edges=False,
)

plotter = pv.Plotter(off_screen=True)
plotter.add_mesh(mesh, color="whitesmoke", specular=0.5)
plotter.add_mesh(edges, color="gray", line_width=2)
plotter.add_bounding_box(color="black", line_width=1)
plotter.show_bounds(grid='back', location='outer', ticks='both',
                    xtitle='X (mm)', ytitle='Y (mm)', ztitle='Z (mm)')
plotter.view_isometric()
plotter.screenshot("preview_iso.png")
plotter.close()
```

> `xlabel` / `ylabel` / `zlabel` were renamed to `xtitle` / `ytitle` / `ztitle` in PyVista 0.44 and now emit a `PyVistaDeprecationWarning`.

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
from pipeline import three_mf

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
from examples.grid_panel import grid_panel

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

`pipeline.core.verify()` enforces the first two by assertion — one solid, one shell, and OCCT BRep validity (which rejects non-manifold edges, free boundary edges and self-intersecting faces). The third is a construction rule: every generator authors in XY and extrudes along Z, so the flat face is always the XY plane.

`pipeline.core.analytic_volume()` computes the material volume of a two-axis lattice by inclusion-exclusion, and `verify()` compares it against the real solid. A broken slot that lands on a wall face and leaves a membrane fails loudly instead of silently shipping a non-perforated plate. The overlap term is summed from the actual slot intervals, so it stays correct even when the end rails are wide enough that the outermost slots of the two families no longer cross.

Run the tests:

```bash
python -m pipeline.selftest          # CSG generators (CadQuery required)
python -m pipeline.three_mf_selftest # 3MF round-trip (CadQuery required for shape bridges; trimesh-only paths also covered)
python -m pipeline.demo               # render the sample parts into out/
```

## Dependencies

- Python >= 3.9
- `cadquery >= 2.4` (CSG)
- `pyvista >= 0.40` (preview; optional — falls back to CadQuery SVG)
- `trimesh >= 4.0` (3MF read + mesh bridge)
- `numpy` (transitive via trimesh)
- `manifold3d` (optional, only needed for `union_replace`)
- Linux headless: EGL/Mesa (`libegl1`, `libegl-mesa0`, `libgl1-mesa-dri`) is enough; `xvfb` only as a fallback

`generators/three_mf.py` keeps `cadquery` as a *lazy* import so reading/inspecting a Bambu 3MF works with `trimesh` alone — useful for QA scripts and for CI smoke tests that do not need the full OCCT stack.