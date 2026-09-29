"""CSS inline-style parsing helpers for HTML -> PPTX conversion."""

import re


def parse_style(style_str):
    """Parse an inline style string into a dict of lowercase key -> value.

    url('...') / url("...") values are kept intact (they contain no ';').
    """
    styles = {}
    if not style_str:
        return styles
    for chunk in style_str.split(';'):
        chunk = chunk.strip()
        if not chunk or ':' not in chunk:
            continue
        key, _, value = chunk.partition(':')
        styles[key.strip().lower()] = value.strip()
    return styles


def parse_px(value, default=None):
    """Extract a float from a CSS length like '12px', '1.5em'(ignored), '12'."""
    if value is None:
        return default
    match = re.match(r'^\s*(-?\d+(?:\.\d+)?)px', str(value).strip())
    if match:
        return float(match.group(1))
    match = re.match(r'^\s*(-?\d+(?:\.\d+)?)\s*$', str(value).strip())
    if match:
        return float(match.group(1))
    return default


def parse_color(value):
    """Return 'rrggbb' from a CSS color value, or None."""
    if not value:
        return None
    value = value.strip()
    match = re.match(r'^#([0-9a-fA-F]{6})$', value)
    if match:
        return match.group(1).lower()
    match = re.match(r'^#([0-9a-fA-F]{3})$', value)
    if match:
        return ''.join(ch * 2 for ch in match.group(1)).lower()
    named = {
        'black': '000000', 'white': 'ffffff', 'red': 'ff0000',
        'green': '008000', 'lime': '00ff00', 'blue': '0000ff',
        'yellow': 'ffff00', 'gray': '808080', 'grey': '808080',
        'transparent': None,
    }
    return named.get(value.lower())


def parse_border_shorthand(value):
    """Parse 'border: 2px solid #ff0000' / 'border-top: ...' -> (width_px, style, color)."""
    if not value:
        return None, None, None
    tokens = value.split()
    width = style = color = None
    for token in tokens:
        px = parse_px(token)
        if px is not None and width is None:
            width = px
        elif token.lower() in ('solid', 'dashed', 'dotted', 'double', 'none'):
            style = token.lower()
        else:
            color = parse_color(token) or color
    return width, style, color


def parse_background_image(value):
    """Return the url path from a background-image value, or None."""
    if not value:
        return None
    match = re.search(r"url\(['\"]?([^'\")]+)['\"]?\)", value)
    return match.group(1) if match else None


def parse_linear_gradient(value):
    """Parse 'linear-gradient(90deg, #fff 0%, #000 100%)'.

    Returns (angle_deg, [(percent, 'rrggbb'), ...]) or None.
    """
    if not value or 'linear-gradient' not in value:
        return None
    match = re.search(
        r'linear-gradient\(\s*(-?\d+)deg\s*,(.*)\)\s*;?\s*$', value.strip().rstrip(';').strip(),
        re.IGNORECASE | re.DOTALL,
    )
    if not match:
        return None
    angle = float(match.group(1))
    stops = []
    for stop in match.group(2).split(','):
        parts = stop.strip().split()
        if not parts:
            continue
        color = parse_color(parts[0])
        if color is None:
            continue
        percent = 0.0
        if len(parts) > 1:
            pm = re.match(r'^(-?\d+(?:\.\d+)?)%$', parts[1])
            if pm:
                percent = float(pm.group(1))
        stops.append((percent, color))
    if len(stops) < 2:
        return None
    return angle, stops


def parse_rotation(transform_value):
    """Extract clockwise rotation degrees from a transform value, default 0."""
    if not transform_value:
        return 0.0
    match = re.search(r'rotate\(\s*(-?\d+(?:\.\d+)?)deg', transform_value)
    return float(match.group(1)) if match else 0.0


def parse_font_family(value):
    """Pick the first concrete family from a CSS font-family list.

    Generic families (sans-serif, serif, ...) and fallback stacks are skipped.
    """
    if not value:
        return None
    generics = {'sans-serif', 'serif', 'monospace', 'cursive', 'fantasy', 'system-ui'}
    for family in value.split(','):
        family = family.strip().strip('"').strip("'")
        if family and family.lower() not in generics:
            return family
    return None
