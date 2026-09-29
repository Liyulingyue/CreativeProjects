---
name: cadquery-3d-print-designer
description: Parameterized 3D printing design pipeline using CadQuery and PyVista. Generates CSG-based Python models, exports STL files, and renders multi-view off-screen preview images (via PyVista for photorealistic entity/section views or native CadQuery SVG for fallback) for visual feedback and iterative refinement loops. Use when designing 3D printable brackets, enclosures, adapters, fan mounts, NUC mounts, or parametric hardware components.
---

# CadQuery 3D Print Designer

A specialized workflow for generating parametric 3D models for additive manufacturing (3D printing) using CadQuery (Python) and performing visual feedback loops using PyVista or native vector SVG exporters.

## Key Features & Pipeline

1. **Parametric CSG Modeling (CadQuery)**:
   - Constructs clean, manifold (watertight) solids via Constructive Solid Geometry (CSG) operations.
   - Automatically incorporates standard 3D printing clearances (+0.2mm to +0.3mm for screw holes, press-fit slots, and mating parts).
   - Exports slice-ready `.stl` files without non-manifold edges or inverted normals.

2. **Dual-Engine Visual Feedback System**:
   - **PyVista Engine (Primary & Recommended)**: Generates high-quality, off-screen (`off_screen=True`) rendered images. Supports multi-angle rendering (Isometric, Front, Top), mesh edge highlights (`show_edges=True`), physical bounding box scales (`show_bounds=True`), and cross-section slices (`clip_plane`).
   - **CadQuery Native SVG Engine (Zero-Dependency Fallback)**: Exports 2D vector wireframe drawings with hidden-line removal when PyVista is unavailable in the environment.

3. **Iterative Refinement Loop**:
   - Executes CadQuery Python code -> Renders multi-view PNG/SVG preview -> Evaluates visually against user specifications -> Refines parameters/code dynamically.

## When to Trigger

Activate this skill when the user requests parametric 3D modeling for 3D printing, including but not limited to:
- Mounting brackets for Mini PCs (NUC, Raspberry Pi, ITX cases).
- Fan shrouds, air ducts, and adapter plates.
- Enclosures, shells, and custom hardware mounts.
- Requests mentioning CadQuery, PyVista, STL generation, or parametric CAD design.

## Recommended Python Execution Blueprint

### Step 1: CadQuery Modeling & STL Export
```python
import cadquery as cq

# 1. Define Parameters (in mm)
length, width, height = 100.0, 100.0, 20.0
wall_thick = 3.0
hole_diam = 80.0
screw_pitch = 71.5
screw_diam = 4.3  # M4 over-hole clearance

# 2. Build CSG Solid
model = (
    cq.Workplane("XY")
    .box(length, width, height)
    .edges("|Z")
    .fillet(5.0)
    .faces(">Z")
    .workplane()
    .hole(hole_diam)
    .rect(screw_pitch, screw_pitch, forConstruction=True)
    .vertices()
    .hole(screw_diam)
)

# 3. Export STL
cq.exporters.export(model, "output_model.stl")

```

### Step 2: PyVista Off-Screen Multi-View Rendering

```python
import pyvista as pv

# Enable Headless Mode (No UI Pop-ups)
pv.start_xvfb()  # Useful for headless Linux/Docker environments if needed

def render_preview(stl_path="output_model.stl", output_img="preview_iso.png"):
    mesh = pv.read(stl_path)
    
    plotter = pv.Plotter(off_screen=True)
    plotter.add_mesh(mesh, color="whitesmoke", show_edges=True, edge_color="gray", specular=0.5)
    plotter.add_bounding_box(color="black", line_width=1)
    plotter.show_bounds(grid='back', location='outer', ticks='both', xlabel='X (mm)', ylabel='Y (mm)', zlabel='Z (mm)')
    plotter.view_isometric()
    
    plotter.screenshot(output_img)
    plotter.close()

render_preview()

```

### Step 3: Zero-Dependency Fallback (CadQuery Native SVG)

```python
# Use only if PyVista is missing
cq.exporters.export(
    model,
    "preview_fallback.svg",
    opt={
        "width": 800,
        "height": 600,
        "showAxes": True,
        "projectionDir": (1, 1, 1),
        "strokeWidth": 1.2
    }
)

```
