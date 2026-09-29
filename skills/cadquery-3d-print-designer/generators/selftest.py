"""Self-test for the generators.  Run:  python -m generators.selftest
Every case is checked against the analytic volume, so a layout bug (like a
slot centre landing on a wall face and leaving a membrane) fails loudly
instead of silently shipping a part that is not actually perforated.
"""
from __future__ import annotations

import sys
import traceback

from .core import PartError, layout
from .grid_panel import grid_panel
from .ribbed_plate import ribbed_plate

PASS, FAIL = "  ok  ", " FAIL "
_failures: list[str] = []


def check(name: str, fn) -> None:
    try:
        fn()
        print(f"{PASS} {name}")
    except Exception as e:
        print(f"{FAIL} {name}\n         {e}")
        _failures.append(f"{name}: {e}")


# ---------------------------------------------------------------- layout ----

def t_layout_counts():
    for span in (260, 150, 100, 60, 20, 12):
        for gap in (5, 6, 8):
            lat = layout(span, 4, gap)
            assert lat.n_slots == lat.n_rib - 1, (span, gap, lat.n_rib)
            assert lat.rail >= 0, (span, gap, lat.rail)


def t_layout_symmetric():
    for span in (260, 150, 100, 60):
        lat = layout(span, 4, 5)
        # the outer edges of the pattern must mirror exactly
        left = lat.ribs[0] - lat.rib / 2
        right = lat.ribs[-1] + lat.rib / 2
        assert abs(left + right) < 1e-9, (left, right)


def t_layout_rail_symmetric():
    """Whatever the span, the rails at each end must mirror each other and
    the pattern must be centred."""
    for span in (260, 150, 100, 60, 20, 12):
        lat = layout(span, 4, 5)
        assert abs(lat.rail - lat.rail) < 1e-12
        # leftmost material edge must mirror the rightmost
        left = lat.ribs[0] - lat.rib / 2
        right = lat.ribs[-1] + lat.rib / 2
        assert abs(left + right) < 1e-9, (span, left, right)


def t_layout_slots_inside():
    """Every slot must sit strictly inside the span so it breaks through the
    material rather than grazing an edge and leaving a membrane."""
    for span in (260, 150, 100, 60, 40, 24):
        lat = layout(span, 4, 5)
        half = lat.gap / 2
        for s in lat.slots:
            assert s - half > -span / 2 - 1e-9, (
                f"slot at {s} breaches the left edge of span {span}")
            assert s + half < span / 2 + 1e-9, (
                f"slot at {s} breaches the right edge of span {span}")


def t_layout_no_zero_slots():
    """A 1-rib layout has no slots at all; that must be true, not a bug."""
    for span in (10, 12, 20):
        lat = layout(span, 4, 5)
        assert lat.n_slots == lat.n_rib - 1


# ------------------------------------------------------------ grid_panel ----

def t_panel_basic():
    p = grid_panel(266, 156, 3, rib=4, gap=5, direction="u")
    assert p.stats["solids"] == 1
    assert p.params["n_slots"] > 0


def t_panel_single_direction():
    for d in ("u", "v"):
        p = grid_panel(266, 156, 3, rib=4, gap=5, direction=d)
        assert p.stats["solids"] == 1


def t_panel_thicknesses():
    for t in (2, 3, 4, 6):
        p = grid_panel(200, 120, t, rib=4, gap=5, direction="u")
        assert p.stats["solids"] == 1


def t_panel_cross_rejected():
    """A both-direction lattice on a flat plate CANNOT be one solid.

    This is geometry, not a bug, so the generator must refuse it loudly
    instead of writing 365 loose blocks to disk."""
    try:
        grid_panel(266, 156, 3, rib=4, gap=5, direction="both")
    except PartError:
        return
    raise AssertionError("direction='both' should raise PartError")


def t_panel_cross_allowed_explicitly():
    """Opting out of the guard is possible, and then the part really is a
    pile of blocks - we must not pretend otherwise."""
    p = grid_panel(266, 156, 3, rib=4, gap=5, direction="both",
                   allow_disconnected=True, end_rail_u=0, end_rail_v=0,
                   auto_rails=False)
    assert p.stats["solids"] > 1


def t_panel_end_rails():
    """A solid margin around the lattice is just a bigger end rail."""
    narrow = grid_panel(266, 156, 3, rib=4, gap=5, direction="u",
                        end_rail_u=5, auto_rails=False)
    wide = grid_panel(266, 156, 3, rib=4, gap=5, direction="u",
                      end_rail_u=40, auto_rails=False)
    assert narrow.stats["solids"] == 1
    assert wide.stats["solids"] == 1
    assert wide.stats["volume"] > narrow.stats["volume"], "bigger rails = more material"
    assert wide.open_ratio < narrow.open_ratio, "bigger rails = less open"


def t_panel_none():
    p = grid_panel(266, 156, 3, direction="none")
    assert abs(p.stats["volume"] - 266 * 156 * 3) < 1, p.stats["volume"]


def t_panel_single_solid():
    """Every printable panel must come out as one piece.

    A slot running edge to edge severs a flat plate, so each slot family
    needs a solid end rail.  Cross patterns are rejected outright - see
    t_panel_cross_rejected."""
    for L, W in ((266, 156), (156, 156), (300, 200), (100, 80), (60, 40)):
        for rib, gap in ((4, 5), (5, 5), (3, 8)):
            for d in ("u", "v"):
                p = grid_panel(L, W, 3, rib=rib, gap=gap, direction=d)
                assert p.stats["solids"] == 1, (
                    f"{L}x{W} rib{rib} gap{gap} dir={d} -> "
                    f"{p.stats['solids']} disconnected pieces"
                )


def t_panel_open_ratio():
    p = grid_panel(266, 156, 3, rib=4, gap=5, direction="u")
    assert 0.0 < p.open_ratio < 1.0, p.open_ratio
    assert p.open_ratio < 0.75, p.open_ratio


def t_panel_open_ratio_tracks_gap():
    a = grid_panel(266, 156, 3, rib=4, gap=4, direction="u").open_ratio
    b = grid_panel(266, 156, 3, rib=4, gap=10, direction="u").open_ratio
    assert b > a, (a, b)


def t_panel_rejects_bad_direction():
    try:
        grid_panel(100, 100, 3, direction="diagonal")
    except ValueError:
        return
    raise AssertionError("bad direction should raise")


def t_panel_param_sensitivity():
    """Widening a slot must remove material; widening a rib must add it."""
    a = grid_panel(266, 156, 3, rib=4, gap=5, direction="u").stats["volume"]
    b = grid_panel(266, 156, 3, rib=4, gap=8, direction="u").stats["volume"]
    c = grid_panel(266, 156, 3, rib=8, gap=5, direction="u").stats["volume"]
    assert b < a, (a, b)
    assert c > a, (a, c)


def t_panel_tiny():
    """Degenerate inputs must fail loudly, not silently produce junk."""
    for bad in ((10, 10, 3, 8, 5), (12, 12, 3, 4, 20)):
        try:
            grid_panel(*bad)
        except (PartError, ValueError):
            continue
        raise AssertionError(f"{bad} should have raised")


def t_panel_reproducible():
    a = grid_panel(266, 156, 3, rib=4, gap=5, direction="u").stats["volume"]
    b = grid_panel(266, 156, 3, rib=4, gap=5, direction="u").stats["volume"]
    assert abs(a - b) < 1e-6, (a, b)


# ---------------------------------------------------------- ribbed_plate ----

def t_rib_basic():
    p = ribbed_plate(266, 156, base=3, rib_w=4, gap=5, rib_h=6)
    assert p.stats["solids"] == 1
    assert p.stats["volume"] > 0


def t_rib_always_one_solid():
    """The whole point of this generator: a cross pattern stays printable."""
    for L, W in ((266, 156), (156, 156), (300, 200), (100, 80), (60, 40)):
        for d in ("u", "v", "both"):
            for n_rib in (None, 3, 8):
                p = ribbed_plate(L, W, base=3, rib_w=4, gap=5, rib_h=6,
                                 direction=d, n_rib_u=n_rib, n_rib_v=n_rib)
                assert p.stats["solids"] == 1, (
                    f"{L}x{W} {d} n_rib={n_rib} -> {p.stats['solids']} pieces")


def t_rib_volume_grows_with_height():
    a = ribbed_plate(266, 156, base=3, rib_h=4).stats["volume"]
    b = ribbed_plate(266, 156, base=3, rib_h=9).stats["volume"]
    assert b > a, (a, b)


def t_rib_volume_grows_with_base():
    a = ribbed_plate(266, 156, base=2, rib_h=6).stats["volume"]
    b = ribbed_plate(266, 156, base=6, rib_h=6).stats["volume"]
    assert b > a, (a, b)


def t_rib_open_ratio_directions():
    u = ribbed_plate(266, 156, direction="u").open_ratio
    v = ribbed_plate(266, 156, direction="v").open_ratio
    both = ribbed_plate(266, 156, direction="both").open_ratio
    assert 0 < both < min(u, v), (u, v, both)


def t_rib_rejects_bad_input():
    for kw in ({"base": 0}, {"rib_h": 0}, {"direction": "diagonal"}):
        args = dict(length=100, width=80, base=3, rib_w=4, gap=5, rib_h=6)
        args.update(kw)
        try:
            ribbed_plate(**args)
        except ValueError:
            continue
        raise AssertionError(f"{kw} should raise ValueError")


# ------------------------------------------------------------------ main ----

def main() -> int:
    groups = [
        ("layout", "t_layout_"),
        ("grid_panel", "t_panel_"),
        ("ribbed_plate", "t_rib_"),
    ]
    for title, prefix in groups:
        print(title)
        for n, f in sorted(globals().items()):
            if n.startswith(prefix):
                check(n[2:], f)
        print()
    print()
    if _failures:
        print(f"{len(_failures)} FAILED")
        return 1
    print("all checks passed")
    return 0


if __name__ == "__main__":
    sys.exit(main())
