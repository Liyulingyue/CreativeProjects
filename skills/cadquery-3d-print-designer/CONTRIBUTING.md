# Contributing to `cadquery-3d-print-designer`

This skill is a **library of parametric part generators**, not a collection of
one-off scripts. The rule for what belongs in the skill is simple:

> A generator takes plain numbers and returns a verified, watertight solid.
> Anything that only makes sense for one specific part, one specific print, or
> one specific session does not belong here.

Read this before adding a file, moving a file, or deleting one.

---

## The two directories

| directory | what it is | who uses it |
| --- | --- | --- |
| `pipeline/` | the skill's **engines**: manifold verification, the PyVista renderer, the 3MF unpack/splice/repack round-trip | the AI, when it needs to verify, preview, or splice geometry |
| `examples/` | **reference implementations**: six complete part generators, each demonstrating one modelling technique | the AI, as a template for writing a new generator |

The split matters. `three_mf.py` is a core capability of this skill — it would
be wrong for it to live in a folder called `examples`. The table-tennis paddle
is *not* a core capability; it is a worked example.

There is deliberately **no `__init__.py`**. Both directories are implicit
namespace packages, so imports are explicit about where a thing lives:

```python
from pipeline.core import verify
from pipeline.render import render
from pipeline.three_mf import load
from examples.grid_panel import grid_panel
```

The skill root must be on `sys.path` for this to work — run from the skill
directory, or add it to `PYTHONPATH`.

---

## What belongs in `pipeline/`

### 1. A shared engine

Code the generators need in order to do their job, and that is not specific
to any one part:

- `core.py` — lattice layout, manifold verification, analytic volume
- `render.py` — the PyVista multi-view renderer
- `three_mf.py` — the 3MF unpack / splice / repack engine

These are imported by the examples, so they must stay dependency-light and must
not import the examples back.

### 2. A self-test

`selftest.py` and `three_mf_selftest.py` prove the engines and the generators
actually work. **If you add an example, add a test.** A generator without a
test is a liability — the next person to touch it has no way to know what
"correct" looked like.

### 3. A demo that exercises the generators

`demo.py` renders the sample parts into `out/` so a human can eyeball them.
Keep it short: one render per part, not one render per parameter sweep.

---

## What belongs in `examples/`

### 1. A parametric part generator

The unit of this skill. It must have **all** of these properties:

| property | why |
| --- | --- |
| Takes plain numbers (a `@dataclass` spec) | so the AI can call it from a description, and a human can call it from a script |
| Returns a verified watertight solid | `verify()` is called inside the generator, not by the caller |
| Is independent of the chassis / the task | it only knows how to make *one* good part |
| Has a `validate()` that rejects impossible input | a bad spec should raise `ValueError`, not produce a broken solid |
| Is deterministic | same numbers → same solid, every time |
| Is documented with a docstring + a parameter table | the AI reads this to decide whether to use it |

### 2. Demonstrates a technique worth copying

An example earns its place by teaching a technique. The current six each show
something different:

| example | technique |
| --- | --- |
| `grid_panel.py` | lattice layout + analytic-volume cross-check |
| `ribbed_plate.py` | keeping a grid one solid |
| `hanger_hook.py` | sweeping a cavity along a curve |
| `blade.py` | width profile → PCHIP spline, mitred inward offset |
| `paddle.py` | a multi-part assembly with a dovetail joint |
| `longcat_logo.py` | SVG path parsing, colour splits, a domed 3D head |

**If your new example does not teach a technique, it does not need to be here.**

---

## What does NOT belong anywhere in this skill

### 1. One-off scripts

A script that builds *one specific thing* for *one specific request* is not a
generator. It has no reuse value and it dilutes the library.

**Do not add:** `bracket_for_the_nuc_on_my_desk.py`,
`render_the_thing_the_user_asked_for.py`.

**Instead:** if the part is genuinely reusable, turn it into an example with a
spec. If it is not, write it in the user's project directory, not here.

### 2. Versioned copies of the same generator

`longcat_logo_v2.py`, `longcat_logo_v3.py`, `longcat_logo_v4.py`, … are six
files that each contain a near-copy of the same SVG parsing, the same
`save_split`, and a slightly different head. That is not six generators; it
is one generator with a messy history.

**Rule:** when a design evolves, **fold the new build into the same file** as a
new function (`longcat_logo()`, `longcat_logo_split()`, `longcat_logo_3d()`),
or replace the old function. Do not add a `_v5` file.

The only exception is when two builds are genuinely different products that
will be maintained separately — e.g. `blade.py` and `paddle.py`: a one-piece
blade and a quick-release paddle are different things, not versions.

### 3. Generated output

`out/` is **not** source. It is build output: STLs, PNGs, 3MFs. It is
gitignored, and it should never be committed.

**Do not add:** anything under `out/`, any `.stl`, any `.png`, any `.3mf`.

### 4. Rendering scripts that only wrap one generator

`longcat_assembly_render.py` existed only to render the longcat parts in
colour. That is what `render()` is for. If you need a special render, write
it as a short script in the user's project, or extend `render()` with a flag.

### 5. Documentation that belongs in `SKILL.md`

The skill's behaviour, pitfalls, and usage patterns go in `SKILL.md` (English,
for the AI) and `SKILL_zh.md` (Chinese, for humans). The code directories are
code.

**SKILL.md is loaded into the AI's context.** Every line costs tokens for
every user. A worked example that only one person cares about belongs in that
example's docstring, not in the skill's main document.

---

## File inventory

| file | kind | keep? |
| --- | --- | --- |
| `pipeline/core.py` | shared engine | yes |
| `pipeline/render.py` | shared engine | yes |
| `pipeline/three_mf.py` | shared engine | yes |
| `pipeline/selftest.py` | self-test | yes |
| `pipeline/three_mf_selftest.py` | self-test | yes |
| `pipeline/demo.py` | demo | yes |
| `pipeline/three_mf_demo.py` | demo | yes |
| `examples/grid_panel.py` | example generator | yes |
| `examples/ribbed_plate.py` | example generator | yes |
| `examples/hanger_hook.py` | example generator | yes |
| `examples/blade.py` | example generator | yes |
| `examples/paddle.py` | example generator | yes |
| `examples/longcat_logo.py` | example generator (3 builds) | yes |
| `out/` | build output | **gitignored** |
| `longcat_logo_v2/v3/v4/v5/v6.py` | versioned copies | **merged into `examples/longcat_logo.py`** |
| `longcat_demo*.py` | one-off scripts | **removed** |
| `longcat_assembly_render.py` | one-off render wrapper | **removed** |
| `generators/__init__.py` | package re-export shim | **removed** — namespace packages, explicit imports |

---

## Adding an example: checklist

1. **Is it reusable?** If you cannot name a second situation where it would be
   useful, it does not belong here.
2. **Does it teach a technique?** If it is just "another part", it is noise.
3. **Does it take a spec?** Plain numbers in, solid out. No globals, no
   reading a config file, no depending on the current working directory.
4. **Does it call `verify()`?** One solid, one shell, valid BRep. If it
   returns a multi-solid result (like the two eyes), use
   `verify(..., require_solid=False)` and say why in a comment.
5. **Does it validate?** `spec.validate()` raises `ValueError` on impossible
   input, before any geometry is built.
6. **Is it deterministic?** No randomness, no time, no network.
7. **Does it have a docstring?** What it is, what the parameters mean, what
   technique it demonstrates, and a short example.
8. **Is there a test?** Add a `t_<name>_*` function to `pipeline/selftest.py`
   and list it in `main()`'s `groups`.
9. **Is it documented?** One row in the examples table in `SKILL.md` (and
   `SKILL_zh.md`). Do **not** add a whole section unless the example teaches
   something the AI will need at design time.
10. **Did you run the tests?** `python -m pipeline.selftest` and
    `python -m pipeline.three_mf_selftest` must both pass.

---

## Removing something: checklist

1. **Is anything importing it?** `grep -rn "<module>" pipeline/ examples/`.
2. **Is it referenced in a docstring?** `grep -rn "<name>" pipeline/ examples/`.
3. **Is it in `pipeline/selftest.py`?** Remove the test and the `groups` entry.
4. **Is it in `pipeline/demo.py`?** Remove the render block.
5. **Is it in `SKILL.md` / `SKILL_zh.md`?** Remove the row or section.
6. **Did you run the tests?** Both self-tests must pass after the removal.

---

## The two hard rules

1. **A generator must be watertight.** Not "mostly watertight", not "watertight
   after a repair pass". If OCCT's tessellation leaves a crack (the sphere
   pole is the classic case), build the shape differently — loft closed wires.
2. **A generator must not depend on another generator's internals.** Use the
   public function or the shared engine. If two examples need the same helper,
   lift the helper into `pipeline/core.py` or into the module that owns the
   concept (e.g. `_logo_polygons` lives in `examples/longcat_logo.py`, not in
   `core.py`).

Break either rule and the skill becomes a pile of scripts that only the
original author can run.
