from .converters import color_to_hex

# Namespaces shared by the theme/background extractors
NS = {
    'p': 'http://schemas.openxmlformats.org/presentationml/2006/main',
    'r': 'http://schemas.openxmlformats.org/officeDocument/2006/relationships',
    'a': 'http://schemas.openxmlformats.org/drawingml/2006/main',
}


def _solid_fill_color(fill_elem):
    """Return hex color string from a solidFill element, or None."""
    srgb = fill_elem.find('.//a:srgbClr', NS)
    if srgb is not None and srgb.get('val'):
        return f"#{srgb.get('val').lower()}"
    scheme = fill_elem.find('.//a:schemeClr', NS)
    if scheme is not None and scheme.get('val'):
        return None  # resolved by caller via theme if needed
    return None


def _picture_fill_image(fill_elem, part):
    """Return (bytes, ext) for a blipFill element, or None."""
    blip = fill_elem.find('.//a:blip', NS)
    if blip is None:
        return None
    r_embed = blip.get('{http://schemas.openxmlformats.org/officeDocument/2006/relationships}embed')
    if not r_embed:
        return None
    try:
        img = part.related_part(r_embed)
        return (img.blob, img.extension)
    except Exception:
        return None


def _gradient_fill_css(fill_elem, prs):
    """Build a CSS linear-gradient from a gradFill element, or None."""
    stops = fill_elem.findall('.//a:gsLst/a:gs', NS)
    if not stops:
        return None
    stop_css = []
    for gs in stops:
        pos = gs.get('pos')
        try:
            pct = float(pos) / 1000.0 if pos else 0.0
        except (TypeError, ValueError):
            pct = 0.0
        srgb = gs.find('.//a:srgbClr', NS)
        color = None
        if srgb is not None and srgb.get('val'):
            color = f"#{srgb.get('val')}"
        else:
            scheme = gs.find('.//a:schemeClr', NS)
            if scheme is not None and scheme.get('val'):
                color = get_scheme_color(prs, scheme.get('val'))
                color = f"#{color}" if color else None
        if color:
            stop_css.append(f"{color} {pct:.0f}%")
    if len(stop_css) < 2:
        return None
    angle = 90
    lin = fill_elem.find('.//a:lin', NS)
    if lin is not None and lin.get('ang'):
        try:
            # OOXML angle is in 60000ths of a degree
            angle = int(int(lin.get('ang')) / 60000) % 360
        except (TypeError, ValueError):
            angle = 90
    return f"background-image: linear-gradient({angle}deg, {', '.join(stop_css)});"


def _bg_from_element(bg, part, prs):
    """Resolve a p:bg element into (background_style, bg_image)."""
    if bg is None:
        return None, None
    bg_pr = bg.find('.//p:bgPr', NS)
    if bg_pr is not None:
        # picture fill
        blip_fill = bg_pr.find('./a:blipFill', NS)
        if blip_fill is not None:
            img = _picture_fill_image(blip_fill, part)
            if img:
                return "picture", img
        # gradient fill
        grad_fill = bg_pr.find('./a:gradFill', NS)
        if grad_fill is not None:
            css = _gradient_fill_css(grad_fill, prs)
            if css:
                return css, None
        # solid fill
        solid = bg_pr.find('./a:solidFill', NS)
        if solid is not None:
            srgb = solid.find('./a:srgbClr', NS)
            if srgb is not None and srgb.get('val'):
                return f"background-color: #{srgb.get('val').lower()};", None
            scheme = solid.find('./a:schemeClr', NS)
            if scheme is not None and scheme.get('val'):
                color = get_scheme_color(prs, scheme.get('val'))
                if color:
                    return f"background-color: #{color.lower()};", None
        return None, None
    # bgRef (theme-defined background)
    bg_ref = bg.find('./p:bgRef', NS)
    if bg_ref is not None:
        scheme = bg_ref.find('.//a:schemeClr', NS)
        if scheme is not None and scheme.get('val'):
            color = get_scheme_color(prs, scheme.get('val'))
            if color:
                return f"background-color: #{color.lower()};", None
    return None, None


def get_background_style(slide, prs):
    """Extract background style from slide, layout, or master.

    Returns a tuple (background_style, bg_image) where bg_image is
    (bytes, ext) when the background is a picture fill, otherwise None.
    The caller is responsible for writing bg_image to the media directory
    and building the final CSS with the correct relative path.
    """
    # 1. Slide-level background
    try:
        bg = slide._element.find('./p:cSld/p:bg', NS)
        style, img = _bg_from_element(bg, slide.part, prs)
        if style:
            return style, img
    except Exception:
        pass

    # 2. Layout-level background
    try:
        layout = slide.slide_layout
        bg = layout._element.find('./p:cSld/p:bg', NS)
        style, img = _bg_from_element(bg, layout.part, prs)
        if style:
            return style, img
    except Exception:
        pass

    # 3. Master-level background
    try:
        master = slide.slide_layout.slide_master
        bg = master._element.find('./p:cSld/p:bg', NS)
        style, img = _bg_from_element(bg, master.part, prs)
        if style:
            return style, img
    except Exception:
        pass

    # 4. Fallback: python-pptx solid fill API (covers more exotic cases)
    try:
        from pptx.enum.dml import MSO_FILL
        fill = slide.background.fill
        if fill.type == MSO_FILL.SOLID:
            rgb = color_to_hex(fill.fore_color)
            if rgb:
                return f"background-color: {rgb};", None
    except Exception:
        pass

    return "background-color: #ffffff;", None


def get_scheme_color(prs, scheme_name):
    """Extract scheme color hex value from the presentation theme."""
    # Map slide background scheme slots to their theme color entries
    color_scheme_map = {
        'bg1': 'lt1',
        'bg2': 'lt2',
        'tx1': 'dk1',
        'tx2': 'dk2',
    }
    color_elem_name = color_scheme_map.get(scheme_name, scheme_name)
    try:
        for rel in prs.part.rels.values():
            if 'theme' not in rel.reltype.lower():
                continue
            from lxml import etree
            theme_elem = etree.fromstring(rel.target_part.blob)
            # Some themes name slots dk1/lt1, others dkFill/ltFill
            candidates = [color_elem_name]
            if color_elem_name == 'lt1':
                candidates.append('ltFill')
            elif color_elem_name == 'lt2':
                candidates.append('ltFill')
            elif color_elem_name == 'dk1':
                candidates.append('dkFill')
            elif color_elem_name == 'dk2':
                candidates.append('dkFill')
            for name in candidates:
                color_elem = theme_elem.find(f'.//a:clrScheme/a:{name}', NS)
                if color_elem is None:
                    continue
                srgb = color_elem.find('./a:srgbClr', NS)
                if srgb is not None and srgb.get('val'):
                    return srgb.get('val')
                sys_clr = color_elem.find('./a:sysClr', NS)
                if sys_clr is not None and sys_clr.get('lastClr'):
                    return sys_clr.get('lastClr')
    except Exception:
        pass
    return None


def resolve_theme_color(prs, theme_color):
    """Resolve an MSO_THEME_COLOR enum member to a '#rrggbb' string via the theme.

    Returns None when the color cannot be resolved.
    """
    try:
        from pptx.enum.dml import MSO_THEME_COLOR
        mapping = {
            'ACCENT_1': 'accent1', 'ACCENT_2': 'accent2', 'ACCENT_3': 'accent3',
            'ACCENT_4': 'accent4', 'ACCENT_5': 'accent5', 'ACCENT_6': 'accent6',
            'DARK_1': 'dk1', 'DARK_2': 'dk2', 'LIGHT_1': 'lt1', 'LIGHT_2': 'lt2',
            'TEXT_1': 'dk1', 'TEXT_2': 'dk2',
            'BACKGROUND_1': 'lt1', 'BACKGROUND_2': 'lt2',
            'HYPERLINK': 'hlink', 'FOLLOWED_HYPERLINK': 'hlinkFollow',
        }
        name = mapping.get(getattr(theme_color, 'name', None))
        if not name:
            return None
        hexval = get_scheme_color(prs, name)
        return f"#{hexval.lower()}" if hexval else None
    except Exception:
        return None


def get_theme_fonts(prs):
    """Return (major_font, minor_font) from the theme, or (None,None)"""
    try:
        from lxml import etree
        for rel in prs.part.rels.values():
            if 'theme' not in rel.reltype.lower():
                continue
            theme_elem = etree.fromstring(rel.target_part.blob)
            major = theme_elem.find('.//a:fontScheme/a:majorFont/a:latin', NS)
            minor = theme_elem.find('.//a:fontScheme/a:minorFont/a:latin', NS)
            return (
                major.get('typeface') if major is not None else None,
                minor.get('typeface') if minor is not None else None,
            )
    except Exception:
        pass
    return None, None
