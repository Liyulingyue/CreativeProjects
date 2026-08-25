from .converters import color_to_hex

def get_background_style(slide, prs):
    """Extract background style from slide, layout, or master"""
    background_style = "background-color: #ffffff;"  # default white
    
    namespaces = {
        'p': 'http://schemas.openxmlformats.org/presentationml/2006/main',
        'r': 'http://schemas.openxmlformats.org/officeDocument/2006/relationships',
        'a': 'http://schemas.openxmlformats.org/drawingml/2006/main'
    }
    
    try:
        # Try direct slide background
        fill = slide.background.fill
        if fill.type == 1:  # solid fill
            if hasattr(fill.fore_color, 'rgb') and fill.fore_color.rgb:
                rgb = fill.fore_color.rgb
                return f"background-color: #{rgb[0]:02x}{rgb[1]:02x}{rgb[2]:02x};"
        elif fill.type == 3:  # picture fill
            if hasattr(fill, 'blip'):
                try:
                    bg_img_bytes = fill.blip.blob
                    bg_ext = fill.blip.ext
                    return f"picture:{bg_img_bytes},{bg_ext}"
                except:
                    pass
    except:
        pass
    
    # Try master slide background via XML
    try:
        master = slide.slide_layout.slide_master
        master_elem = master._element
        cSld = master_elem.find('.//p:cSld', namespaces)
        if cSld is not None:
            bg = cSld.find('.//p:bg', namespaces)
            if bg is not None:
                # Check for direct fill elements
                solidFill = bg.find('.//a:solidFill', namespaces)
                if solidFill is not None:
                    srgbClr = solidFill.find('.//a:srgbClr', namespaces)
                    if srgbClr is not None:
                        color_val = srgbClr.get('val')
                        if color_val:
                            return f"background-color: #{color_val};"
                
                blipFill = bg.find('.//a:blipFill', namespaces)
                if blipFill is not None:
                    blip = blipFill.find('.//a:blip', namespaces)
                    if blip is not None:
                        r_embed = blip.get('{http://schemas.openxmlformats.org/officeDocument/2006/relationships}embed')
                        if r_embed:
                            try:
                                img = master.part.related_part(r_embed)
                                bg_img_bytes = img.blob
                                return f"picture:{bg_img_bytes},{img.extension}"
                            except:
                                pass
                
                # Try scheme color reference
                bgRef = bg.find('.//p:bgRef', namespaces)
                if bgRef is not None:
                    schemeClr = bgRef.find('.//a:schemeClr', namespaces)
                    if schemeClr is not None:
                        scheme_val = schemeClr.get('val')
                        if scheme_val:
                            # Get color from theme
                            color_hex = get_scheme_color(prs, scheme_val)
                            if color_hex:
                                return f"background-color: #{color_hex};"
    except:
        pass
    
    return background_style

def get_scheme_color(prs, scheme_name):
    """Extract scheme color from theme"""
    try:
        prs_part = prs.part
        rels = prs_part.rels
        
        for rel_id, rel in rels.items():
            if 'theme' in rel.reltype.lower():
                theme_xml = rel.target_part.blob
                from lxml import etree
                theme_elem = etree.fromstring(theme_xml)
                
                namespaces = {
                    'a': 'http://schemas.openxmlformats.org/drawingml/2006/main'
                }
                
                # Map scheme name to color scheme
                color_scheme_map = {
                    'bg1': 'ltFill',  # light fill
                    'bg2': 'dk2Fill',  # dark fill
                    'accent1': 'accent1',
                }
                
                # Find the color scheme
                color_elem_name = color_scheme_map.get(scheme_name, scheme_name)
                color_elem = theme_elem.find(f'.//a:clrScheme/a:{color_elem_name}', namespaces)
                if color_elem is not None:
                    srgbClr = color_elem.find('.//a:srgbClr', namespaces)
                    if srgbClr is not None:
                        return srgbClr.get('val')
    except:
        pass
    
    return None

def get_theme_fonts(prs):
    """Return (major_font, minor_font) from the theme, or (None,None)"""
    try:
        prs_part = prs.part
        rels = prs_part.rels
        from lxml import etree
        for rel_id, rel in rels.items():
            if 'theme' in rel.reltype.lower():
                theme_xml = rel.target_part.blob
                theme_elem = etree.fromstring(theme_xml)
                ns = {'a': 'http://schemas.openxmlformats.org/drawingml/2006/main'}
                # majorFont latin typeface
                major = None
                minor = None
                major_elem = theme_elem.find('.//a:fontScheme/a:majorFont/a:latin', ns)
                if major_elem is not None:
                    major = major_elem.get('typeface')
                minor_elem = theme_elem.find('.//a:fontScheme/a:minorFont/a:latin', ns)
                if minor_elem is not None:
                    minor = minor_elem.get('typeface')
                return major, minor
    except Exception:
        pass
    return None, None