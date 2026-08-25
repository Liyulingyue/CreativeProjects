from .themes import get_theme_fonts, get_scheme_color

def get_effective_font(run, paragraph, shape, theme_minor_font, layout_default=None, default_is_title=False):
    """Return a tuple (font_family, font_size_pt) by checking run, paragraph, shape, fallback to theme and defaults.
    default_is_title: if True choose larger default font-size for title placeholders/guesses."""
    # Font family fallback
    font_family = None
    try:
        if run is not None and run.font is not None and getattr(run.font, 'name', None):
            font_family = run.font.name
        elif paragraph is not None and getattr(paragraph, 'font', None) is not None and getattr(paragraph.font, 'name', None):
            font_family = paragraph.font.name
        elif shape is not None and getattr(shape, 'text_frame', None) is not None and getattr(shape.text_frame, 'paragraphs', None):
            p = shape.text_frame.paragraphs[0]
            if getattr(p, 'font', None) and getattr(p.font, 'name', None):
                font_family = p.font.name
    except Exception:
        font_family = None
    if not font_family:
        if layout_default and layout_default.get('font_family'):
            font_family = layout_default.get('font_family')
        else:
            font_family = theme_minor_font or 'Arial'

    # Font size fallback
    size_pt = None
    try:
        if run is not None and run.font is not None and getattr(run.font, 'size', None):
            size_pt = run.font.size.pt
        elif paragraph is not None and getattr(paragraph, 'font', None) and getattr(paragraph.font, 'size', None):
            size_pt = paragraph.font.size.pt
        elif shape is not None and getattr(shape, 'text_frame', None) is not None and getattr(shape.text_frame.paragraphs[0], 'font', None) and getattr(shape.text_frame.paragraphs[0].font, 'size', None):
            size_pt = shape.text_frame.paragraphs[0].font.size.pt
    except Exception:
        size_pt = None
    if not size_pt:
        if layout_default and layout_default.get('font_size_pt'):
            size_pt = layout_default.get('font_size_pt')
        else:
            size_pt = 40 if default_is_title else 18

    return font_family, size_pt

def get_layout_placeholder_defaults(layout, prs=None):
    """Extract default text run properties for placeholders in this layout.
    Returns a dict: {placeholder_type: {font_family, font_size_pt, color, bold, italic, underline}}"""
    defaults = {}
    try:
        from lxml import etree
        ns = {'a': 'http://schemas.openxmlformats.org/drawingml/2006/main', 'p': 'http://schemas.openxmlformats.org/presentationml/2006/main'}
        layout_elem = layout._element
        # For each shape in layout, check if it's a placeholder
        for lshape in layout.shapes:
            try:
                if not getattr(lshape, 'is_placeholder', False):
                    continue
                ph_type = None
                try:
                    ph_type = lshape.placeholder_format.type
                except Exception:
                    ph_type = None
                # default structure
                style = {'font_family': None, 'font_size_pt': None, 'color': None, 'bold': None, 'italic': None, 'underline': None}
                elem = lshape._element
                # Check dinside a:txBody/a:p/a:r/a:rPr
                rPr = elem.find('.//a:txBody/a:p/a:r/a:rPr', ns)
                if rPr is None:
                    # Check for defRPr under a:pPr or a:lstStyle
                    rPr = elem.find('.//a:txBody/a:p/a:pPr/a:defRPr', ns)
                if rPr is None:
                    # also check shape-specific lstStyle lvl1..lvl9
                    for lvl_idx in range(1, 10):
                        lvl_tag = f'.//a:txBody/a:lstStyle//a:lvl{lvl_idx}pPr/a:defRPr'
                        rPr = elem.find(lvl_tag, ns)
                        if rPr is not None:
                            break
                if rPr is None:
                    # find in layout lvl defaults lvl1..lvl9
                    for lvl_idx in range(1, 10):
                        lvl_tag = f'.//a:lstStyle//a:lvl{lvl_idx}pPr/a:defRPr'
                        layout_elem.find(lvl_tag, ns)
                        if rPr is not None:
                            break
                if rPr is not None:
                    # size in 'sz' (in hundredth of a point), e.g., 2800 => 28.0 pt
                    sz = rPr.get('sz')
                    if sz:
                        try:
                            style['font_size_pt'] = float(sz) / 100.0
                        except Exception:
                            pass
                    # check bold/italic/underline attrs
                    if rPr.get('b') in ('1', 'true'):
                        style['bold'] = True
                    if rPr.get('i') in ('1', 'true'):
                        style['italic'] = True
                    if rPr.get('u') not in (None, 'none'):
                        style['underline'] = True
                    # check for color
                    sf = rPr.find('.//a:solidFill/a:srgbClr', ns)
                    if sf is not None and sf.get('val'):
                        style['color'] = f"#{sf.get('val')}"
                    else:
                        sc = rPr.find('.//a:solidFill/a:schemeClr', ns)
                        if sc is not None and sc.get('val') and prs is not None:
                            col = get_scheme_color(prs, sc.get('val'))
                            if col:
                                style['color'] = f"#{col}"
                    # check latin typeface
                    latin = rPr.find('.//a:latin', ns)
                    if latin is not None and latin.get('typeface'):
                        style['font_family'] = latin.get('typeface')
                # if not found, fallback to slide master title/body styles
                if (style.get('font_size_pt') is None or style.get('font_family') is None or style.get('color') is None) and prs is not None:
                    try:
                        master = layout.slide_master
                        master_elem = master._element
                        # try txStyles title or body
                        for style_type in ['title', 'body']:
                            title_rpr = master_elem.find(f'.//a:txStyles/a:{style_type}/a:lvl1pPr/a:defRPr', ns)
                            if title_rpr is not None:
                                if style.get('font_size_pt') is None and title_rpr.get('sz'):
                                    try:
                                        style['font_size_pt'] = float(title_rpr.get('sz')) / 100.0
                                    except Exception:
                                        pass
                                if style.get('color') is None:
                                    srgb = title_rpr.find('.//a:srgbClr', ns)
                                    if srgb is not None and srgb.get('val'):
                                        style['color'] = f"#{srgb.get('val')}"
                                    else:
                                        sc = title_rpr.find('.//a:schemeClr', ns)
                                        if sc is not None and sc.get('val') and prs is not None:
                                            col = get_scheme_color(prs, sc.get('val'))
                                            if col:
                                                style['color'] = f"#{col}"
                                if style.get('font_family') is None:
                                    latin = title_rpr.find('.//a:latin', ns)
                                    if latin is not None and latin.get('typeface'):
                                        style['font_family'] = latin.get('typeface')
                    except Exception:
                        pass

                # If font_family is an alias like +mn-lt or +maj-lt, map it to theme fonts
                try:
                    if style.get('font_family') and style.get('font_family').startswith('+') and prs is not None:
                        maj, mino = get_theme_fonts(prs)
                        val = style.get('font_family')
                        if 'mn' in val:
                            style['font_family'] = mino or style['font_family']
                        elif 'maj' in val:
                            style['font_family'] = maj or style['font_family']
                except Exception:
                    pass
                defaults[ph_type] = style
            except Exception:
                pass
    except Exception:
        pass
    return defaults