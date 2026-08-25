from pptx.enum.dml import MSO_LINE_DASH_STYLE

def emu_to_px(emu):
    # Approximate conversion: 1 inch = 914400 EMU, 1 inch = 96 px
    return int(emu * 96 / 914400)

def emu_to_pt(emu):
    # 1 pt = 12700 EMU
    try:
        return emu / 12700
    except Exception:
        return None

def color_to_hex(rgb_obj):
    try:
        rgb = rgb_obj.rgb
        return f"#{rgb[0]:02x}{rgb[1]:02x}{rgb[2]:02x}"
    except Exception:
        return None

def pt_to_px(pt):
    try:
        return int(pt * 96.0 / 72.0)
    except Exception:
        return None

def dash_style_to_css(dash_style):
    try:
        # Map pptx dash style to CSS border-style
        if dash_style is None:
            return 'solid'
        mapping = {
            MSO_LINE_DASH_STYLE.SOLID: 'solid',
            MSO_LINE_DASH_STYLE.DASH: 'dashed',
            MSO_LINE_DASH_STYLE.DOT: 'dotted',
            MSO_LINE_DASH_STYLE.DASH_DOT: 'dashed',
            MSO_LINE_DASH_STYLE.LONG_DASH: 'dashed',
            MSO_LINE_DASH_STYLE.DASH_DOT_DOT: 'dashed',
        }
        return mapping.get(dash_style, 'solid')
    except Exception:
        return 'solid'