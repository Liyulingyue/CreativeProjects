import math
import os
import html as html_module
from pptx.enum.shapes import MSO_SHAPE_TYPE
from pptx.enum.shapes import PP_PLACEHOLDER
from pptx.enum.dml import MSO_FILL, MSO_COLOR_TYPE
from pptx.enum.text import MSO_VERTICAL_ANCHOR
from pptx.oxml.ns import qn
from .converters import emu_to_px, emu_to_pt, color_to_hex, pt_to_px
from .fonts import get_effective_font
from .themes import resolve_theme_color

_NUMBER_FORMATS = {
    'arabicPeriod': '{n}.',
    'arabicParenR': '{n})',
    'arabicParenBoth': '({n})',
    'alphaLcPeriod': '{a}.',
    'alphaUcPeriod': '{A}.',
    'alphaLcParenR': '{a})',
    'alphaUcParenR': '{A})',
}


def html_builder():
    """Create a simple html line builder for pretty printing."""
    lines = []
    def add(line, indent=0):
        lines.append(('    ' * indent) + line)
    def to_str(compact=False):
        if compact:
            return ''.join(line.lstrip() for line in lines)
        return '\n'.join(lines)
    return add, to_str


def escape_text(text):
    """Escape text for safe inclusion in HTML."""
    return html_module.escape(text or '', quote=False)


def pick_default_text_color(background_style):
    """Choose a readable default text color based on the background color.

    Parses the background-color from the style string and uses its
    relative luminance; falls back to dark text on an unknown/light
    background and white on a dark background.
    """
    import re
    default_color = '#1f2937'  # near-black for unknown/light backgrounds
    if not background_style:
        return default_color
    match = re.search(r'background-color:\s*#([0-9a-fA-F]{6})', background_style)
    if not match:
        return default_color
    hexval = match.group(1)
    r, g, b = (int(hexval[i:i + 2], 16) for i in (0, 2, 4))
    luminance = 0.2126 * r + 0.7152 * g + 0.0722 * b
    return '#ffffff' if luminance < 128 else default_color


def _readable_color_on(bg_hex, fallback='#1f2937'):
    """Pick black/white text for a 'rrggbb' background hex, fallback if unknown."""
    if not bg_hex or len(bg_hex) != 6:
        return fallback
    try:
        r, g, b = (int(bg_hex[i:i + 2], 16) for i in (0, 2, 4))
    except ValueError:
        return fallback
    luminance = 0.2126 * r + 0.7152 * g + 0.0722 * b
    return '#ffffff' if luminance < 128 else fallback


def walk_shapes(shapes, transform=None):
    """Yield (shape, left, top, width, height) in slide EMU coordinates.

    Recurses into group shapes, applying the group's chOff/chExt child
    coordinate mapping and accumulating nested group transforms.
    """
    if transform is None:
        transform = lambda x, y, w, h: (x, y, w, h)
    for sh in shapes:
        if sh.shape_type == MSO_SHAPE_TYPE.GROUP:
            params = _group_transform_params(sh)
            if params is None:
                continue
            off_x, off_y, choff_x, choff_y, sx, sy = params
            def child_tx(x, y, w, h, _params=params, _outer=transform):
                _off_x, _off_y, _chx, _chy, _sx, _sy = _params
                gx = _off_x + (x - _chx) * _sx
                gy = _off_y + (y - _chy) * _sy
                return _outer(gx, gy, w * _sx, h * _sy)
            yield from walk_shapes(sh.shapes, child_tx)
        else:
            try:
                left, top, width, height = sh.left, sh.top, sh.width, sh.height
            except Exception:
                continue
            if left is None:
                continue
            yield (sh,) + transform(left, top, width or 0, height or 0)


def _group_transform_params(group_shape):
    """Extract (off, chOff, scale) from a group shape's xfrm, or None."""
    try:
        xfrm = group_shape._element.find(f"{qn('p:grpSpPr')}/{qn('a:xfrm')}")
        if xfrm is None:
            return None

        def attr(tag, name):
            el = xfrm.find(qn(tag))
            return int(el.get(name)) if el is not None and el.get(name) else 0

        off_x, off_y = attr('a:off', 'x'), attr('a:off', 'y')
        choff_x, choff_y = attr('a:chOff', 'x'), attr('a:chOff', 'y')
        ext_x, ext_y = attr('a:ext', 'cx'), attr('a:ext', 'cy')
        chext_x, chext_y = attr('a:chExt', 'cx'), attr('a:chExt', 'cy')
        sx = ext_x / chext_x if chext_x else 1.0
        sy = ext_y / chext_y if chext_y else 1.0
        return off_x, off_y, choff_x, choff_y, sx, sy
    except Exception:
        return None


def _outer_shadow_css(shape):
    """Build a CSS box-shadow from an outerShdw effect, or ''."""
    try:
        # spPr lives in the p: namespace; its effect children in a:
        shdw = shape._element.find(
            f".//{qn('p:spPr')}/{qn('a:effectLst')}/{qn('a:outerShdw')}"
        )
        if shdw is None:
            return ''
        blur_px = emu_to_px(int(shdw.get('blurRad') or 0))
        dist = int(shdw.get('dist') or 0)
        direction = int(shdw.get('dir') or 0) / 60000.0
        dx_px = dist * math.cos(math.radians(direction))
        dy_px = dist * math.sin(math.radians(direction))
        srgb = shdw.find(qn('a:srgbClr'))
        color = srgb.get('val') if srgb is not None else '000000'
        alpha = 1.0
        if srgb is not None:
            alpha_el = srgb.find(qn('a:alpha'))
            if alpha_el is not None and alpha_el.get('val'):
                alpha = max(0.0, min(1.0, int(alpha_el.get('val')) / 100000.0))
        r, g, b = (int(color[i:i + 2], 16) for i in (0, 2, 4))
        return (f"box-shadow: {dx_px:.1f}px {dy_px:.1f}px {blur_px}px "
                f"rgba({r}, {g}, {b}, {alpha:.2f}); ")
    except Exception:
        return ''


def _autofit_font_scale(text_frame):
    """Read normAutofit fontScale (0-1) from the text body, default 1.0."""
    try:
        body_pr = text_frame._txBody.find(qn('a:bodyPr'))
        if body_pr is None:
            return 1.0
        norm = body_pr.find(qn('a:normAutofit'))
        if norm is None or not norm.get('fontScale'):
            return 1.0
        return max(0.05, min(1.0, int(norm.get('fontScale').rstrip('%')) / 100.0))
    except Exception:
        return 1.0


def _bullet_info(paragraph):
    """Return (kind, value, marL_emu, indent_emu) for a paragraph's bullet.

    kind is 'char', 'num' or None.
    """
    try:
        p_pr = paragraph._p.find(qn('a:pPr'))
        if p_pr is None:
            return None, None, 0, 0
        if p_pr.find(qn('a:buNone')) is not None:
            return None, None, 0, 0
        bu_char = p_pr.find(qn('a:buChar'))
        if bu_char is not None:
            return 'char', bu_char.get('char') or '\u2022', \
                int(p_pr.get('marL') or 0), int(p_pr.get('indent') or 0)
        bu_num = p_pr.find(qn('a:buAutoNum'))
        if bu_num is not None:
            return 'num', bu_num.get('type') or 'arabicPeriod', \
                int(p_pr.get('marL') or 0), int(p_pr.get('indent') or 0)
    except Exception:
        pass
    return None, None, 0, 0


def _bullet_prefix(kind, value, counter):
    """Render the bullet marker text for a paragraph."""
    if kind == 'char':
        return escape_text(value) + '&nbsp;'
    if kind == 'num':
        fmt = _NUMBER_FORMATS.get(value, '{n}.')
        if '{a}' in fmt or '{A}' in fmt:
            letters = 'abcdefghijklmnopqrstuvwxyz'
            letter = letters[(counter - 1) % 26] if counter > 0 else 'a'
            if '{A}' in fmt:
                letter = letter.upper()
            return escape_text(fmt.replace('{a}', letter).replace('{A}', letter)) + '&nbsp;'
        return escape_text(fmt.format(n=counter)) + '&nbsp;'
    return ''


def _line_height_css(paragraph):
    """Build a CSS line-height declaration from paragraph.line_spacing."""
    try:
        spacing = paragraph.line_spacing
        if spacing is None:
            return ''
        if isinstance(spacing, float) or isinstance(spacing, int):
            return f"line-height: {float(spacing)}; "
        # Length (EMU) -> px
        return f"line-height: {emu_to_px(int(spacing))}px; "
    except Exception:
        return ''


def _run_color(run, prs, fallback):
    """Resolve a run's text color: RGB -> theme -> fallback."""
    try:
        color = run.font.color
        if color is None or color.type is None:
            return fallback
        if color.type == MSO_COLOR_TYPE.RGB:
            return color_to_hex(color) or fallback
        if color.type == MSO_COLOR_TYPE.SCHEME:
            return resolve_theme_color(prs, color.theme_color) or fallback
    except Exception:
        pass
    return fallback


def _render_cell_runs(cell, prs, default_color):
    """Render a table cell's text paragraphs with basic run styling."""
    paragraphs_html = []
    for paragraph in cell.text_frame.paragraphs:
        para_style = ''
        if paragraph.alignment:
            align_map = {1: "left", 2: "center", 3: "right", 4: "justify"}
            para_style += f"text-align: {align_map.get(int(paragraph.alignment), 'left')}; "
        runs_html = ''
        for run in paragraph.runs:
            run_style = ''
            if run.font.size:
                run_style += f"font-size: {pt_to_px(run.font.size.pt) or int(run.font.size.pt)}px; "
            if run.font.bold:
                run_style += "font-weight: bold; "
            if run.font.italic:
                run_style += "font-style: italic; "
            if run.font.underline:
                run_style += "text-decoration: underline; "
            run_color = _run_color(run, prs, None)
            if run_color:
                run_style += f"color: {run_color}; "
            else:
                run_style += f"color: {default_color}; "
            if run.font.name:
                run_style += f"font-family: {run.font.name}; "
            runs_html += f'<span style="{run_style}">{escape_text(run.text)}</span>'
        if not runs_html:
            continue
        paragraphs_html.append(f'<p style="{para_style}">{runs_html}</p>')
    return ''.join(paragraphs_html)


def _render_table_html(shape, prs):
    """Render a table shape as a styled HTML table."""
    table = shape.table
    col_widths = [emu_to_px(c.width) if c.width else 40 for c in table.columns]
    row_heights = [emu_to_px(r.height) if r.height else 24 for r in table.rows]
    parts = [
        f'<table style="table-layout: fixed; width: {sum(col_widths)}px; border-collapse: collapse;">',
        '<colgroup>' + ''.join(f'<col style="width: {w}px">' for w in col_widths) + '</colgroup>',
    ]
    for row_idx, row in enumerate(table.rows):
        parts.append(f'<tr style="height: {row_heights[row_idx]}px;">')
        for cell in row.cells:
            if cell.is_spanned:
                continue
            span_attrs = ''
            if cell.span_height > 1:
                span_attrs += f' rowspan="{cell.span_height}"'
            if cell.span_width > 1:
                span_attrs += f' colspan="{cell.span_width}"'
            cell_style = ''
            fill_color = None
            try:
                if cell.fill.type == MSO_FILL.SOLID:
                    fill_color = color_to_hex(cell.fill.fore_color)
                    if fill_color:
                        cell_style += f"background-color: {fill_color}; "
            except Exception:
                pass
            try:
                anchor = cell.vertical_anchor
                if anchor == MSO_VERTICAL_ANCHOR.MIDDLE:
                    cell_style += "vertical-align: middle; "
                elif anchor == MSO_VERTICAL_ANCHOR.BOTTOM:
                    cell_style += "vertical-align: bottom; "
            except Exception:
                pass
            for margin_attr, css_prop in (
                ('margin_left', 'padding-left'), ('margin_right', 'padding-right'),
                ('margin_top', 'padding-top'), ('margin_bottom', 'padding-bottom'),
            ):
                try:
                    margin = getattr(cell, margin_attr)
                    if margin:
                        cell_style += f"{css_prop}: {emu_to_px(int(margin))}px; "
                except Exception:
                    pass
            default_color = _readable_color_on(fill_color)
            content = _render_cell_runs(cell, prs, default_color)
            parts.append(f'<td{span_attrs} style="{cell_style}">{content}</td>')
        parts.append('</tr>')
    parts.append('</table>')
    return ''.join(parts)


_CHART_TYPE_NAMES = {
    'COLUMN_CLUSTERED': 'column', 'BAR_CLUSTERED': 'bar', 'LINE': 'line',
    'LINE_MARKERS': 'line', 'PIE': 'pie', 'DOUGHNUT': 'doughnut',
    'AREA': 'area',
}


def _render_chart_html(shape, prs):
    """Render a chart graphic frame as a structured data table.

    Browsers cannot render native OOXML charts, so the chart's data is
    emitted as a .chart-data table; the reverse converter rebuilds a
    native chart from it.
    """
    try:
        chart = shape.chart
    except Exception:
        return '<div class="chart">[Chart]</div>'
    type_name = _CHART_TYPE_NAMES.get(str(getattr(chart, 'chart_type', '')).split(' ')[0], 'column')
    title = ''
    try:
        if chart.has_title:
            title = chart.chart_title.text_frame.text
    except Exception:
        pass
    categories = []
    series = []
    try:
        plot = chart.plots[0]
        categories = [str(c) if c is not None else '' for c in plot.categories]
        for s in plot.series:
            series.append((str(s.name or ''), list(s.values or [])))
    except Exception:
        pass
    parts = [f'<div class="chart" data-chart-type="{type_name}">']
    if title:
        parts.append(f'<div class="chart-title">{escape_text(title)}</div>')
    parts.append('<table class="chart-data">')
    parts.append('<tr><th></th>' + ''.join(f'<th>{escape_text(c)}</th>' for c in categories) + '</tr>')
    for name, values in series:
        cells = ''.join(f'<td>{v if v is not None else ""}</td>' for v in values)
        parts.append(f'<tr><th>{escape_text(name)}</th>{cells}</tr>')
    parts.append('</table></div>')
    return ''.join(parts)


def _is_smart_art(shape):
    """True when the graphic frame holds a SmartArt diagram."""
    try:
        graphic_data = shape._element.find(f'.//{qn("a:graphic")}/{qn("a:graphicData")}')
        return graphic_data is not None and 'diagram' in (graphic_data.get('uri') or '')
    except Exception:
        return False


def _freeform_svg(shape, width_px, height_px):
    """Render a freeform shape's custom geometry as an inline SVG polygon."""
    try:
        cust_geom = shape._element.find(f".//{qn('p:spPr')}/{qn('a:custGeom')}")
        if cust_geom is None:
            return '[Freeform]'
        path = cust_geom.find(f"{qn('a:pathLst')}/{qn('a:path')}")
        if path is None:
            return '[Freeform]'
        path_w = int(path.get('w') or 0) or max(1, width_px)
        path_h = int(path.get('h') or 0) or max(1, height_px)
        points = []
        for cmd in path:
            tag = cmd.tag
            if tag in (qn('a:moveTo'), qn('a:lnTo')):
                pt = cmd.find(qn('a:pt'))
                if pt is not None and pt.get('x') and pt.get('y'):
                    x = int(pt.get('x')) / path_w * width_px
                    y = int(pt.get('y')) / path_h * height_px
                    points.append(f"{x:.1f},{y:.1f}")
        if len(points) < 2:
            return '[Freeform]'
        fill_color = 'transparent'
        try:
            if shape.fill.type == MSO_FILL.SOLID:
                fill_color = color_to_hex(shape.fill.fore_color) or 'transparent'
        except Exception:
            pass
        stroke = 'none'
        stroke_width = 0
        try:
            if shape.line.color and shape.line.color.rgb:
                stroke = color_to_hex(shape.line.color) or 'none'
            if shape.line.width:
                stroke_width = max(1, int(emu_to_pt(shape.line.width) / 1.333))
        except Exception:
            pass
        return (
            f'<svg class="freeform" width="{width_px}" height="{height_px}" '
            f'viewBox="0 0 {width_px} {height_px}">'
            f'<polygon points="{" ".join(points)}" style="fill: {fill_color}; '
            f'stroke: {stroke}; stroke-width: {stroke_width}px;" /></svg>'
        )
    except Exception:
        return '[Freeform]'


def _slide_transition_css(slide):
    """CSS fade-in animation when the slide defines a fade transition."""
    try:
        trans = slide._element.find(f".//{qn('p:transition')}")
        if trans is not None and trans.find(qn('p:fade')) is not None:
            return ('@keyframes slideFadeIn { from { opacity: 0; } to { opacity: 1; } }\n'
                    '            ')
    except Exception:
        pass
    return ''


def generate_index_html(filename_base, num_slides, html_dir, compact):
    """Generate index.html for the presentation slides."""
    add_idx, to_str_idx = html_builder()
    add_idx('<!DOCTYPE html>')
    add_idx('<html lang="zh-CN">')
    add_idx('<head>', 1)
    add_idx('<meta charset="UTF-8">', 2)
    add_idx('<title>PPT Slides Index</title>', 2)
    add_idx('</head>', 1)
    add_idx('<body>', 1)
    add_idx('<h1>幻灯片列表</h1>', 2)
    add_idx('<ul>', 2)
    for i in range(1, num_slides + 1):
        add_idx(f'<li><a href="slides/slide{i}.html">Slide {i}</a></li>', 3)
    add_idx('</ul>', 2)
    add_idx('</body>', 1)
    add_idx('</html>')
    with open(os.path.join(html_dir, f"{filename_base}_index.html"), 'w', encoding='utf-8') as f:
        f.write(to_str_idx(compact=compact))

def generate_main_html(source_dir, html_dir, compact):
    """Generate main.html entry page (links to per-file subdirectories)."""
    main_add, main_to_str = html_builder()
    main_add('<!DOCTYPE html>')
    main_add('<html lang="zh-CN">')
    main_add('<head>', 1)
    main_add('<meta charset="UTF-8">', 2)
    main_add('<title>PPT to HTML Converter - Main Entry</title>', 2)
    main_add('<style>', 2)
    main_add('body { font-family: Arial, sans-serif; margin: 40px; background-color: #f4f4f4; }', 3)
    main_add('.container { max-width: 800px; margin: 0 auto; background: white; padding: 20px; border-radius: 8px; box-shadow: 0 0 10px rgba(0,0,0,0.1); }', 3)
    main_add('h1 { color: #333; }', 3)
    main_add('ul { list-style-type: none; padding: 0; }', 3)
    main_add('li { margin: 10px 0; }', 3)
    main_add('a { text-decoration: none; color: #007bff; font-size: 18px; }', 3)
    main_add('a:hover { text-decoration: underline; }', 3)
    main_add('</style>', 2)
    main_add('</head>', 1)
    main_add('<body>', 1)
    main_add('<div class="container">', 2)
    main_add('<h1>PPT to HTML 转换结果</h1>', 3)
    main_add('<p>以下是转换后的演示文稿：</p>', 3)
    main_add('<ul>', 3)
    for filename in os.listdir(source_dir):
        if filename.endswith('.pptx'):
            filename_base = os.path.splitext(filename)[0]
            main_add(f'<li><a href="{filename_base}/{filename_base}_index.html">{filename_base}</a></li>', 4)
    main_add('</ul>', 3)
    main_add('<p>点击链接查看幻灯片。</p>', 3)
    main_add('</div>', 2)
    main_add('</body>', 1)
    main_add('</html>')
    with open(os.path.join(html_dir, 'main.html'), 'w', encoding='utf-8') as f:
        f.write(main_to_str(compact=compact))

def generate_slide_html(i, num_slides, theme_minor_font, slide_width_px, slide_height_px, background_style, nav, layout_images_filtered, layout_shapes, slide, prs, layout_placeholder_defaults, html_dir, compact, media_dir=None, media_prefix=None):
    """Generate HTML for a single slide.

    html_dir is the directory the slide HTML file is written to; media_dir
    is where extracted media (images/videos) are saved. When media_dir is
    not provided it is derived from html_dir's sibling 'media' directory.
    media_prefix namespaces media filenames per source presentation.
    """
    if media_dir is None:
        media_dir = os.path.join(os.path.dirname(html_dir), "media")
    prefix = f"{media_prefix}_" if media_prefix else ""
    add, to_str = html_builder()
    # Prepare fallback font-family: theme minor font -> Chinese fallback -> Arial -> sans-serif
    default_font_stack = []
    if theme_minor_font:
        default_font_stack.append(theme_minor_font)
    # Add common Chinese fonts and system fonts for better fidelity
    default_font_stack.extend(["微软雅黑", "Microsoft YaHei", "Helvetica", "Arial", "sans-serif"])
    default_font_family = ', '.join([f'"{f}"' for f in default_font_stack])
    # top part
    add('<!DOCTYPE html>')
    add('<html lang="zh-CN">')
    add('<head>', 1)
    add('<meta charset="UTF-8">', 2)
    add(f'<title>Slide {i}</title>', 2)
    add('<style>', 2)
    add(f'body {{ font-family: {default_font_family}; padding: 20px; }}', 3)
    transition_css = _slide_transition_css(slide)
    if transition_css:
        add(transition_css.rstrip() + ' .slide { animation: slideFadeIn 0.7s ease-out; }', 3)
    add(f'.slide {{ position: relative; width: {slide_width_px}px; height: {slide_height_px}px; {background_style} border: 1px solid #ccc; margin: 0 auto; box-sizing: border-box; overflow: hidden; }}', 3)
    add('.shape { position: absolute; z-index: 2; box-sizing: border-box; }', 3)
    add('.layout-image { position: absolute; z-index: 0; }', 3)
    add('.layout-shape { position: absolute; z-index: 1; box-sizing: border-box; }', 3)
    add('.shape img { display: block; object-fit: contain; }', 3)
    add('* { -webkit-font-smoothing: antialiased; text-rendering: optimizeLegibility; }', 3)
    add('p { line-height: 1.15; margin: 0; }', 3)
    add('table { border-collapse: collapse; }', 3)
    add('td, th { border: 1px solid #000; padding: 4px; }', 3)
    add('.nav { text-align: center; margin-bottom: 20px; }', 3)
    add('</style>', 2)
    add('</head>', 1)
    add('<body>', 1)
    add(nav, 2)
    add('<div class="slide">', 2)
    add('<!-- layout/master images -->', 3)
    # Append layout images (non-full-slide)
    for lfname, lleft, ltop, lw, lh in layout_images_filtered:
        lstyle = f"left: {lleft}px; top: {ltop}px; width: {lw}px; height: {lh}px;"
        add(f'<div class="shape layout-image" style="{lstyle}"><img src="{lfname}" style="width: 100%; height: 100%;" alt="Background Image"></div>', 3)
    # render layout shapes (lines / auto shapes)
    for lshape in layout_shapes:
        try:
            stype = lshape.get('type')
            sleft = lshape.get('left')
            stop = lshape.get('top')
            sw = lshape.get('width')
            sh = lshape.get('height')
            srot = lshape.get('rotation', 0)
            if stype == MSO_SHAPE_TYPE.LINE:
                stroke_width = lshape.get('stroke_width', 2)
                stroke_color = lshape.get('stroke_color', '#000') or '#000'
                dash_style = lshape.get('dash_style') if lshape.get('dash_style', None) else 'solid'
                sstyle = f"left: {sleft}px; top: {stop}px; width: {sw}px; height: {max(1, stroke_width)}px; transform-origin: left top; transform: rotate({srot}deg);"
                # use border-top for dashed style; if dashed, set border-top style else fill
                css_border = f"border-top: {stroke_width}px {dash_style} {stroke_color};"
                add(f'<div class="shape layout-shape line" style="{sstyle} {css_border}"></div>', 3)
            elif stype == MSO_SHAPE_TYPE.AUTO_SHAPE:
                fill_color = lshape.get('fill_color', 'transparent') or 'transparent'
                stroke_color = lshape.get('stroke_color')
                stroke_width = lshape.get('stroke_width')
                border_style = ''
                if stroke_color and stroke_width:
                    border_style = f"border: {stroke_width}px solid {stroke_color};"
                sstyle = f"left: {sleft}px; top: {stop}px; width: {sw}px; height: {sh}px; background-color: {fill_color}; {border_style}; transform-origin: left top; transform: rotate({srot}deg);"
                add(f'<div class="shape layout-shape auto-shape" style="{sstyle}"></div>', 3)
        except Exception:
            pass

    img_count = 0
    for shape, left_emu, top_emu, width_emu, height_emu in walk_shapes(slide.shapes):
        left_px = emu_to_px(left_emu)
        top_px = emu_to_px(top_emu)
        width_px = emu_to_px(width_emu)
        height_px = emu_to_px(height_emu)
        shape_style = f"left: {left_px}px; top: {top_px}px; width: {width_px}px; height: {height_px}px;"

        if shape.shape_type == MSO_SHAPE_TYPE.PICTURE:
            # Handle images
            image = shape.image
            image_bytes = image.blob
            ext = image.ext
            img_filename = f"{prefix}slide{i}_img{img_count}.{ext}"
            with open(os.path.join(media_dir, img_filename), 'wb') as f:
                f.write(image_bytes)
            shadow_css = _outer_shadow_css(shape)
            rot = getattr(shape, 'rotation', 0) or 0
            if rot:
                shape_style += f" transform: rotate({rot}deg); transform-origin: left top;"
            add(f'<div class="shape" style="{shape_style} {shadow_css}"><img src="../media/{img_filename}" style="width: 100%; height: 100%;" alt="Image"></div>', 3)
            img_count += 1
        elif shape.shape_type == MSO_SHAPE_TYPE.TABLE:
            add(f'<div class="shape" style="{shape_style}">{_render_table_html(shape, prs)}</div>', 3)
        elif getattr(shape, 'has_chart', False):
            add(f'<div class="shape" style="{shape_style}">{_render_chart_html(shape, prs)}</div>', 3)
        elif shape.shape_type == MSO_SHAPE_TYPE.FREEFORM:
            add(f'<div class="shape" style="{shape_style}">{_freeform_svg(shape, width_px, height_px)}</div>', 3)
        elif _is_smart_art(shape):
            add(f'<div class="shape" style="{shape_style}"><div style="width: 100%; height: 100%; background: #eef2ff; border: 1px dashed #93a3d0; display: flex; align-items: center; justify-content: center;">[SmartArt]</div></div>', 3)
        elif shape.shape_type == MSO_SHAPE_TYPE.MEDIA:
            # Handle videos
            try:
                # Find video relationship
                video_rel = None
                for rel in shape.part.rels.values():
                    if 'video' in rel.reltype or 'media' in rel.reltype:
                        video_rel = rel
                        break

                if video_rel and video_rel.target_part and hasattr(video_rel.target_part, 'blob'):
                    # Extract video file
                    video_bytes = video_rel.target_part.blob
                    # Determine file extension from content type or filename
                    ext = 'mp4'  # default
                    if hasattr(video_rel.target_part, 'content_type'):
                        if 'mp4' in video_rel.target_part.content_type:
                            ext = 'mp4'
                        elif 'avi' in video_rel.target_part.content_type:
                            ext = 'avi'
                        elif 'mov' in video_rel.target_part.content_type:
                            ext = 'mov'
                        elif 'wmv' in video_rel.target_part.content_type:
                            ext = 'wmv'

                    video_filename = f"{prefix}slide{i}_video{img_count}.{ext}"
                    with open(os.path.join(media_dir, video_filename), 'wb') as f:
                        f.write(video_bytes)

                    # Generate video HTML with poster frame if available
                    poster_attr = ""
                    if shape.poster_frame:
                        poster_bytes = shape.poster_frame.blob
                        poster_ext = shape.poster_frame.ext
                        poster_filename = f"{prefix}slide{i}_poster{img_count}.{poster_ext}"
                        with open(os.path.join(media_dir, poster_filename), 'wb') as f:
                            f.write(poster_bytes)
                        poster_attr = f' poster="../media/{poster_filename}"'

                    video_html = f'<video controls style="width: 100%; height: 100%;"{poster_attr}><source src="../media/{video_filename}" type="video/{ext}">Your browser does not support the video tag.</video>'
                    add(f'<div class="shape" style="{shape_style}">{video_html}</div>', 3)
                    img_count += 1
            except Exception:
                # Fallback: just show a placeholder
                add(f'<div class="shape" style="{shape_style}"><div style="width: 100%; height: 100%; background: #f0f0f0; display: flex; align-items: center; justify-content: center; border: 1px solid #ccc;">[Video]</div></div>', 3)
        elif hasattr(shape, "text_frame") and shape.text_frame and (
            shape.shape_type != MSO_SHAPE_TYPE.AUTO_SHAPE or shape.text_frame.text.strip()
        ):
            # Handle text shapes with full styling; autoshapes with real text
            # also land here so their text styling is preserved
            text_html = ""
            try:
                font_scale = _autofit_font_scale(shape.text_frame)
                shadow_css = _outer_shadow_css(shape)
                rot = getattr(shape, 'rotation', 0) or 0
                if rot:
                    shape_style += f" transform: rotate({rot}deg); transform-origin: left top;"
                number_counters = {}
                for paragraph in shape.text_frame.paragraphs:
                    para_style = ""
                    # Get paragraph level properties
                    if paragraph.alignment:
                        # PP_ALIGN: LEFT=1, CENTER=2, RIGHT=3, JUSTIFY=4
                        align_map = {1: "left", 2: "center", 3: "right", 4: "justify"}
                        para_style += f"text-align: {align_map.get(int(paragraph.alignment), 'left')}; "
                    # indentation for bullet/levels
                    try:
                        level = getattr(paragraph, 'level', 0) or 0
                        if level and level > 0:
                            para_style += f"margin-left: {level * 28}px; "
                    except Exception:
                        pass

                    # bullets / numbered lists from pPr
                    bullet_kind, bullet_value, marL_emu, indent_emu = _bullet_info(paragraph)
                    if bullet_kind:
                        if marL_emu:
                            para_style += f"margin-left: {max(0, emu_to_px(marL_emu))}px; "
                        if indent_emu:
                            para_style += f"text-indent: {emu_to_px(indent_emu)}px; "
                        key = marL_emu
                        if bullet_kind == 'num':
                            number_counters[key] = number_counters.get(key, 0) + 1
                            bullet_html = _bullet_prefix(bullet_kind, bullet_value, number_counters[key])
                        else:
                            bullet_html = _bullet_prefix(bullet_kind, bullet_value, 0)
                    else:
                        bullet_html = ''

                    para_style += _line_height_css(paragraph)

                    para_html = f'<p style="{para_style}">'
                    if bullet_html:
                        para_html += f'<span>{bullet_html}</span>'
                    for run in paragraph.runs:
                        run_style = ""
                        # detect title placeholder heuristics for default size
                        try:
                            default_is_title = False
                            if getattr(shape, 'is_placeholder', False):
                                ph = shape.placeholder
                                if ph is not None and getattr(ph, 'placeholder_format', None) is not None:
                                    ptype = getattr(ph.placeholder_format, 'type', None)
                                    if ptype in (PP_PLACEHOLDER.TITLE, PP_PLACEHOLDER.CENTER_TITLE):
                                        default_is_title = True
                            # also use spatial heuristic: top near top and text short
                            if not default_is_title:
                                if top_px is not None and top_px < (slide_height_px * 0.18) and len(run.text.strip()) < 25:
                                    default_is_title = True
                        except Exception:
                            default_is_title = False
                        # get effective font family and size based on layout fallback
                        layout_defaults = None
                        try:
                            lid = id(shape.slide.slide_layout)
                            layout_defaults = layout_placeholder_defaults.get(lid, {}).get(getattr(shape.placeholder_format, 'type', None), None)
                        except Exception:
                            layout_defaults = None
                        ff, fsize = get_effective_font(run, paragraph, shape, theme_minor_font, layout_default=layout_defaults, default_is_title=default_is_title)
                        if fsize and font_scale != 1.0:
                            fsize = float(fsize) * font_scale
                        if fsize:
                            fsize_px = pt_to_px(fsize) or int(fsize)
                            run_style += f"font-size: {fsize_px}px; "
                            # approximate line-height based on font size
                            try:
                                line_h_px = int(float(fsize) * 1.15)
                                run_style += f"line-height: {line_h_px}px; "
                            except Exception:
                                pass
                        if ff:
                            run_style += f"font-family: {ff}; "
                        if run.font.bold:
                            run_style += "font-weight: bold; "
                        elif layout_defaults and layout_defaults.get('bold'):
                            run_style += "font-weight: bold; "
                        if run.font.italic:
                            run_style += "font-style: italic; "
                        elif layout_defaults and layout_defaults.get('italic'):
                            run_style += "font-style: italic; "
                        if run.font.underline:
                            run_style += "text-decoration: underline; "
                        elif layout_defaults and layout_defaults.get('underline'):
                            run_style += "text-decoration: underline; "
                        # Extract text color: RGB / theme scheme color / layout defaults
                        text_color = _run_color(run, prs, None)

                        # Third try: layout defaults
                        if text_color is None and layout_defaults and layout_defaults.get('color'):
                            text_color = layout_defaults.get('color')

                        # Apply the color if found, else fall back to a
                        # readable default based on the slide background
                        if text_color:
                            run_style += f"color: {text_color}; "
                        else:
                            run_style += f"color: {pick_default_text_color(background_style)}; "
                        if run.font.name:
                            run_style += f"font-family: {run.font.name}; "
                        span_html = f'<span style="{run_style}">{escape_text(run.text)}</span>'
                        # hyperlinks
                        try:
                            href = run.hyperlink.address
                            if href:
                                span_html = f'<a href="{html_module.escape(href, quote=True)}" style="color: inherit;">{span_html}</a>'
                        except Exception:
                            pass
                        para_html += span_html
                    if not paragraph.runs and paragraph.text:
                        # Paragraph without explicit runs (e.g. line breaks / field text)
                        para_html += escape_text(paragraph.text)
                    para_html += '</p>'
                    text_html += para_html
            except Exception:
                # Fallback: just use the text
                text_html = f'<p>{escape_text(shape.text)}</p>'

            add(f'<div class="shape" style="{shape_style} {shadow_css}">{text_html}</div>', 3)
        elif shape.shape_type == MSO_SHAPE_TYPE.LINE:
            # draw a simple line as a thin rectangle with stroke color
            try:
                stroke_color = '#000'
                stroke_width = 2
                if hasattr(shape, 'line') and shape.line is not None:
                    if hasattr(shape.line, 'color') and hasattr(shape.line.color, 'rgb'):
                        stroke_color = color_to_hex(shape.line.color)
                    if shape.line.width:
                        wpt = emu_to_pt(shape.line.width)
                        if wpt:
                            stroke_width = max(1, int(wpt / 1.333))
                rot = getattr(shape, 'rotation', 0) or 0
                sstyle = f"left: {left_px}px; top: {top_px}px; width: {width_px}px; height: {max(1, stroke_width)}px; background-color: {stroke_color}; transform-origin: left top; transform: rotate({rot}deg);"
                add(f'<div class="shape line" style="{sstyle}"></div>', 3)
            except Exception:
                pass
        elif shape.shape_type == MSO_SHAPE_TYPE.AUTO_SHAPE:
            try:
                fill_color = 'transparent'
                stroke_color = None
                stroke_width = None
                if shape.fill and hasattr(shape.fill, 'fore_color') and hasattr(shape.fill.fore_color, 'rgb'):
                    fill_color = color_to_hex(shape.fill.fore_color)
                if hasattr(shape, 'line') and shape.line is not None:
                    if hasattr(shape.line, 'color') and hasattr(shape.line.color, 'rgb'):
                        stroke_color = color_to_hex(shape.line.color)
                    if shape.line.width:
                        wpt = emu_to_pt(shape.line.width)
                        if wpt:
                            stroke_width = max(1, int(wpt / 1.333))
                border_style = ''
                if stroke_color and stroke_width:
                    border_style = f"border: {stroke_width}px solid {stroke_color};"
                rot = getattr(shape, 'rotation', 0) or 0
                sstyle = f"left: {left_px}px; top: {top_px}px; width: {width_px}px; height: {height_px}px; background-color: {fill_color}; {border_style}; transform-origin: left top; transform: rotate({rot}deg);"
                shadow_css = _outer_shadow_css(shape)
                # autoshapes whose text frame is empty are rendered as plain
                # shapes; any text content is escaped into the div
                inner_text = escape_text(shape.text_frame.text) if getattr(shape, 'text_frame', None) and shape.text_frame.text.strip() else ''
                add(f'<div class="shape auto-shape" style="{sstyle} {shadow_css}">{inner_text}</div>', 3)
            except Exception:
                pass

    add('</div>', 2)
    # speaker notes (hidden)
    try:
        if slide.has_notes_slide:
            notes_text = slide.notes_slide.notes_text_frame.text
            if notes_text and notes_text.strip():
                add(f'<div class="notes" style="display: none;">{escape_text(notes_text)}</div>', 2)
    except Exception:
        pass
    add('</body>', 1)
    add('</html>')

    slide_filename = os.path.join(html_dir, f"slide{i}.html")
    with open(slide_filename, 'w', encoding='utf-8') as f:
        f.write(to_str(compact=compact))
