"""Parse generated slide HTML files back into structured shape models.

The parser understands the HTML produced by this library's forward
conversion (pptx -> html): absolutely positioned `.shape` divs inside a
`.slide` container, containing images, tables, styled paragraphs, videos
or bare auto-shape/line divs.
"""

import os

from lxml import html as lxml_html

from .css_utils import (
    parse_style,
    parse_px,
    parse_color,
    parse_border_shorthand,
    parse_background_image,
    parse_linear_gradient,
    parse_rotation,
    parse_font_family,
)


def _geometry(style):
    return (
        parse_px(style.get('left'), 0.0),
        parse_px(style.get('top'), 0.0),
        parse_px(style.get('width'), 0.0),
        parse_px(style.get('height'), 0.0),
        parse_rotation(style.get('transform')),
    )


def _parse_paragraphs(container):
    """Parse <p> elements into paragraph models with styled runs.

    Runs wrapped in <a href="..."> keep the link target in 'href'.
    """
    paragraphs = []
    for p in container.xpath('./p'):
        style = parse_style(p.get('style'))
        align_map = {'left': 'left', 'center': 'center', 'right': 'right', 'justify': 'justify'}
        runs = []
        span_nodes = p.xpath('./span | ./a/span')
        href_nodes = p.xpath('./a')
        hrefs = {id(a): a.get('href') for a in href_nodes}
        for span in span_nodes:
            span_style = parse_style(span.get('style'))
            # find enclosing <a>, if any
            parent = span.getparent()
            href = hrefs.get(id(parent)) if parent is not None else None
            runs.append({
                'text': span.text_content(),
                'font_size_px': parse_px(span_style.get('font-size')),
                'font_family': parse_font_family(span_style.get('font-family')),
                'bold': 'font-weight' in span_style and 'bold' in span_style['font-weight'].lower(),
                'italic': 'font-style' in span_style and 'italic' in span_style['font-style'].lower(),
                'underline': 'text-decoration' in span_style and 'underline' in span_style['text-decoration'].lower(),
                'color': parse_color(span_style.get('color')),
                'href': href,
            })
        if not runs and p.text_content().strip():
            runs.append({'text': p.text_content(), 'font_size_px': None,
                         'font_family': None, 'bold': False, 'italic': False,
                         'underline': False, 'color': None, 'href': None})
        margin_px = parse_px(style.get('margin-left'), 0.0)
        paragraphs.append({
            'align': align_map.get(style.get('text-align', '').strip(), None),
            'margin_px': margin_px,
            'runs': runs,
        })
    return paragraphs


def _parse_table(table_elem):
    """Parse a table into a grid of cell models.

    Returns rows as lists aligned to the table grid; cells covered by a
    rowspan/colspan merge are None. Visible cells carry text, optional
    background color and span information.
    """
    covered = {}
    rows = []
    for r, tr in enumerate(table_elem.xpath('./tr')):
        row_cells = []
        c = 0
        for td in tr.xpath('./td'):
            while covered.get((r, c)):
                row_cells.append(None)
                c += 1
            style = parse_style(td.get('style'))
            cell = {
                'text': td.text_content().replace('\n', ' ').strip(),
                'bg': parse_color(style.get('background-color')),
            }
            rowspan = int(td.get('rowspan') or 1)
            colspan = int(td.get('colspan') or 1)
            if rowspan > 1:
                cell['rowspan'] = rowspan
            if colspan > 1:
                cell['colspan'] = colspan
            for rr in range(r, r + rowspan):
                for cc in range(c, c + colspan):
                    if (rr, cc) != (r, c):
                        covered[(rr, cc)] = True
            row_cells.append(cell)
            c += colspan
        while covered.get((r, c)):
            row_cells.append(None)
            c += 1
        rows.append(row_cells)
    return {'kind': 'table', 'rows': rows}


def _parse_slide_class_style(tree):
    """Extract the `.slide { ... }` rule from the <style> block."""
    for style_elem in tree.xpath('//style'):
        css = style_elem.text_content()
        import re
        match = re.search(r'\.slide\s*\{([^}]*)\}', css)
        if match:
            return parse_style(match.group(1))
    return {}


def parse_slide_html(html_path):
    """Parse one slide HTML file into a slide model dict."""
    with open(html_path, 'rb') as f:
        tree = lxml_html.fromstring(f.read())

    slide_div = tree.xpath('//div[contains(concat(" ", normalize-space(@class), " "), " slide ")]')
    if not slide_div:
        raise ValueError(f"No .slide container found in {html_path}")
    slide_div = slide_div[0]
    # slide geometry/backgrounds live in the .slide CSS rule; an inline
    # style attribute (if present) takes precedence
    style = {**_parse_slide_class_style(tree), **parse_style(slide_div.get('style'))}

    model = {
        'width_px': parse_px(style.get('width'), 960.0),
        'height_px': parse_px(style.get('height'), 720.0),
        'background_color': parse_color(style.get('background-color')),
        'background_image': parse_background_image(style.get('background-image')),
        'background_gradient': parse_linear_gradient(style.get('background-image')),
        'shapes': [],
    }

    for elem in slide_div.xpath('./div'):
        classes = (elem.get('class') or '').split()
        if 'nav' in classes:
            continue
        shape_style = parse_style(elem.get('style'))
        left, top, width, height, rotation = _geometry(shape_style)
        base = {'left': left, 'top': top, 'width': width, 'height': height, 'rotation': rotation}

        # image (standalone or layout background image)
        imgs = elem.xpath('./img')
        if imgs and 'auto-shape' not in classes:
            model['shapes'].append({**base, 'kind': 'image', 'src': imgs[0].get('src')})
            continue

        # table
        tables = elem.xpath('./table')
        if tables:
            shape = {**base, 'kind': 'table'}
            shape.update(_parse_table(tables[0]))
            model['shapes'].append(shape)
            continue

        # video (source + optional poster frame)
        videos = elem.xpath('.//video')
        if videos:
            video = videos[0]
            sources = video.xpath('./source')
            model['shapes'].append({
                **base, 'kind': 'video',
                'src': sources[0].get('src') if sources else None,
                'poster': video.get('poster'),
            })
            continue

        # styled text paragraphs
        if elem.xpath('./p'):
            model['shapes'].append({**base, 'kind': 'text', 'paragraphs': _parse_paragraphs(elem)})
            continue

        # bare line div (thin filled rectangle or border-top drawn line)
        if 'line' in classes:
            stroke_width, dash_style, stroke_color = parse_border_shorthand(shape_style.get('border-top'))
            # slide-level lines are drawn as thin background-filled divs
            if stroke_color is None:
                stroke_color = parse_color(shape_style.get('background-color'))
            model['shapes'].append({
                **base, 'kind': 'line',
                'stroke_color': stroke_color,
                'stroke_width': stroke_width or parse_px(shape_style.get('height'), 1.0),
                'dash_style': dash_style or 'solid',
            })
            continue

        # bare auto-shape div (fill/border rectangle)
        if 'auto-shape' in classes:
            fill_color = parse_color(shape_style.get('background-color'))
            stroke_width, _stroke_style, stroke_color = parse_border_shorthand(shape_style.get('border'))
            text = elem.text_content().strip()
            model['shapes'].append({
                **base, 'kind': 'autoshape',
                'fill_color': fill_color,
                'stroke_color': stroke_color,
                'stroke_width': stroke_width,
                'text': text or None,
            })
            continue

    # speaker notes (hidden div)
    notes = tree.xpath('//div[contains(@class, "notes")]')
    if notes:
        notes_text = notes[0].text_content().strip()
        if notes_text:
            model['notes'] = notes_text

    return model


def resolve_media_path(html_path, src):
    """Resolve a src attribute relative to the slide HTML file location."""
    if not src or os.path.isabs(src) or src.startswith(('http://', 'https://', 'data:')):
        return None
    return os.path.normpath(os.path.join(os.path.dirname(html_path), src))
