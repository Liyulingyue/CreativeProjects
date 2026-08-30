"""Build a PPTX presentation from parsed HTML slide models."""

import io
import os

from pptx import Presentation
from pptx.util import Emu, Pt
from pptx.dml.color import RGBColor
from pptx.enum.text import PP_ALIGN
from pptx.enum.shapes import MSO_SHAPE, MSO_CONNECTOR
from pptx.enum.dml import MSO_LINE_DASH_STYLE

from .converters import px_to_emu
from .html_parsers import resolve_media_path

_ALIGN_MAP = {
    'left': PP_ALIGN.LEFT,
    'center': PP_ALIGN.CENTER,
    'right': PP_ALIGN.RIGHT,
    'justify': PP_ALIGN.JUSTIFY,
}

_DASH_MAP = {
    'solid': MSO_LINE_DASH_STYLE.SOLID,
    'dashed': MSO_LINE_DASH_STYLE.DASH,
    'dotted': MSO_LINE_DASH_STYLE.ROUND_DOT,
}


def _rgb(color):
    return RGBColor.from_string(color)


def _apply_slide_background(prs, slide, model, html_path):
    if model.get('background_image'):
        img_path = resolve_media_path(html_path, model['background_image'])
        if img_path and os.path.isfile(img_path):
            # python-pptx has no picture-background API; draw the image
            # first so it sits at the bottom of the z-order
            slide.shapes.add_picture(
                img_path, 0, 0,
                width=Emu(px_to_emu(model['width_px'])),
                height=Emu(px_to_emu(model['height_px'])),
            )
            return
    if model.get('background_gradient'):
        angle, stops = model['background_gradient']
        fill = slide.background.fill
        fill.gradient()
        for stop, (percent, color) in zip(fill.gradient_stops, stops):
            stop.position = max(0.0, min(1.0, percent / 100.0))
            stop.color.rgb = _rgb(color)
        # CSS angle: 0deg points up; pptx angle: degrees clockwise from right
        try:
            fill.gradient_angle = (angle - 90) % 360
        except Exception:
            pass
        return
    if model.get('background_color'):
        fill = slide.background.fill
        fill.solid()
        fill.fore_color.rgb = _rgb(model['background_color'])


def _add_text_shape(shapes, shape):
    box = shapes.add_textbox(
        Emu(px_to_emu(shape['left'])), Emu(px_to_emu(shape['top'])),
        Emu(px_to_emu(shape['width'])), Emu(px_to_emu(shape['height'])),
    )
    if shape.get('rotation'):
        box.rotation = shape['rotation']
    tf = box.text_frame
    tf.word_wrap = True
    for idx, para in enumerate(shape.get('paragraphs', [])):
        paragraph = tf.paragraphs[0] if idx == 0 else tf.add_paragraph()
        if para.get('align') in _ALIGN_MAP:
            paragraph.alignment = _ALIGN_MAP[para['align']]
        margin_px = para.get('margin_px') or 0
        if margin_px:
            pPr = paragraph._p.get_or_add_pPr()
            pPr.set('marL', str(px_to_emu(margin_px)))
        for run_model in para.get('runs', []):
            run = paragraph.add_run()
            run.text = run_model['text']
            font = run.font
            if run_model.get('font_size_px') is not None:
                font.size = Pt(run_model['font_size_px'] * 72.0 / 96.0)
            if run_model.get('font_family'):
                font.name = run_model['font_family']
            font.bold = bool(run_model.get('bold'))
            font.italic = bool(run_model.get('italic'))
            if run_model.get('underline'):
                font.underline = True
            if run_model.get('color'):
                font.color.rgb = _rgb(run_model['color'])
            if run_model.get('href'):
                try:
                    run.hyperlink.address = run_model['href']
                except Exception:
                    pass
    return box


def _add_image_shape(shapes, shape, html_path):
    img_path = resolve_media_path(html_path, shape['src'])
    if img_path and os.path.isfile(img_path):
        shapes.add_picture(
            img_path, Emu(px_to_emu(shape['left'])), Emu(px_to_emu(shape['top'])),
            width=Emu(px_to_emu(shape['width'])), height=Emu(px_to_emu(shape['height'])),
        )
    else:
        _add_placeholder(shapes, shape, f"[image: {os.path.basename(shape.get('src') or '?')}]")


def _add_table_shape(shapes, shape):
    rows = shape.get('rows') or []
    if not rows:
        return
    n_rows = len(rows)
    n_cols = max(len(row) for row in rows)
    graphic_frame = shapes.add_table(
        n_rows, n_cols,
        Emu(px_to_emu(shape['left'])), Emu(px_to_emu(shape['top'])),
        Emu(px_to_emu(shape['width'])), Emu(px_to_emu(shape['height'])),
    )
    table = graphic_frame.table
    for r, row in enumerate(rows):
        c = 0
        for cell_model in row:
            # grid slots consumed by earlier merges are spanned cells
            while c < n_cols and table.cell(r, c).is_spanned:
                c += 1
            if c >= n_cols:
                break
            if cell_model is not None:
                cell = table.cell(r, c)
                cell.text = cell_model.get('text') or ''
                if cell_model.get('bg'):
                    try:
                        cell.fill.solid()
                        cell.fill.fore_color.rgb = _rgb(cell_model['bg'])
                    except Exception:
                        pass
                rowspan = cell_model.get('rowspan', 1)
                colspan = cell_model.get('colspan', 1)
                if rowspan > 1 or colspan > 1:
                    try:
                        cell.merge(table.cell(
                            min(r + rowspan - 1, n_rows - 1),
                            min(c + colspan - 1, n_cols - 1),
                        ))
                    except Exception:
                        pass
            c += 1


def _add_autoshape(shapes, shape):
    box = shapes.add_shape(
        MSO_SHAPE.RECTANGLE,
        Emu(px_to_emu(shape['left'])), Emu(px_to_emu(shape['top'])),
        Emu(px_to_emu(shape['width'])), Emu(px_to_emu(shape['height'])),
    )
    if shape.get('rotation'):
        box.rotation = shape['rotation']
    if shape.get('fill_color'):
        box.fill.solid()
        box.fill.fore_color.rgb = _rgb(shape['fill_color'])
    else:
        box.fill.background()
    if shape.get('stroke_color') and shape.get('stroke_width'):
        box.line.color.rgb = _rgb(shape['stroke_color'])
        box.line.width = Pt(shape['stroke_width'] * 72.0 / 96.0)
    else:
        box.line.fill.background()
    if shape.get('text'):
        box.text_frame.text = shape['text']
    return box


def _add_line_shape(shapes, shape):
    """Recreate a CSS-drawn line as a straight connector.

    The HTML line is a horizontal div rotated around its left-top origin,
    so the endpoint is computed from width and rotation.
    """
    import math
    angle = math.radians(shape.get('rotation') or 0.0)
    length = shape.get('width') or 0.0
    x1 = px_to_emu(shape['left'])
    y1 = px_to_emu(shape['top'])
    x2 = px_to_emu(shape['left'] + length * math.cos(angle))
    y2 = px_to_emu(shape['top'] + length * math.sin(angle))
    connector = shapes.add_connector(MSO_CONNECTOR.STRAIGHT, Emu(x1), Emu(y1), Emu(x2), Emu(y2))
    if shape.get('stroke_color'):
        connector.line.color.rgb = _rgb(shape['stroke_color'])
    if shape.get('stroke_width'):
        connector.line.width = Pt(shape['stroke_width'] * 72.0 / 96.0)
    dash = _DASH_MAP.get(shape.get('dash_style') or 'solid')
    if dash:
        connector.line.dash_style = dash
    return connector


def _add_video_shape(shapes, shape, html_path):
    video_path = resolve_media_path(html_path, shape.get('src'))
    poster_path = resolve_media_path(html_path, shape.get('poster')) if shape.get('poster') else None
    if video_path and os.path.isfile(video_path):
        try:
            kwargs = {}
            if poster_path and os.path.isfile(poster_path):
                kwargs['poster_frame_image'] = poster_path
            shapes.add_movie(
                video_path, Emu(px_to_emu(shape['left'])), Emu(px_to_emu(shape['top'])),
                width=Emu(px_to_emu(shape['width'])), height=Emu(px_to_emu(shape['height'])),
                mime_type='video/mp4', **kwargs,
            )
            return
        except Exception:
            pass
    _add_placeholder(shapes, shape, '[video]')


def _add_placeholder(shapes, shape, label):
    box = shapes.add_shape(
        MSO_SHAPE.RECTANGLE,
        Emu(px_to_emu(shape['left'])), Emu(px_to_emu(shape['top'])),
        Emu(px_to_emu(shape['width'])), Emu(px_to_emu(shape['height'])),
    )
    box.fill.solid()
    box.fill.fore_color.rgb = RGBColor.from_string('f0f0f0')
    box.line.color.rgb = RGBColor.from_string('cccccc')
    box.text_frame.text = label
    return box


def build_presentation(slide_models, html_dir, output_path, slide_width_px=None, slide_height_px=None):
    """Build and save a PPTX from parsed slide models.

    slide_models: list of slide model dicts from parse_slide_html.
    html_dir: directory containing the slides/ subdirectory (used to
    resolve media paths, which are relative to each slide file).
    """
    prs = Presentation()
    first = slide_models[0] if slide_models else {'width_px': 960.0, 'height_px': 720.0}
    width_px = slide_width_px or first['width_px']
    height_px = slide_height_px or first['height_px']
    prs.slide_width = Emu(px_to_emu(width_px))
    prs.slide_height = Emu(px_to_emu(height_px))

    blank_layout = prs.slide_layouts[6] if len(prs.slide_layouts) > 6 else prs.slide_layouts[0]

    for model in slide_models:
        slide = prs.slides.add_slide(blank_layout)
        html_path = os.path.join(html_dir, 'slides', model['_html_file'])
        _apply_slide_background(prs, slide, model, html_path)
        shapes = slide.shapes
        for shape in model['shapes']:
            kind = shape['kind']
            if kind == 'image':
                _add_image_shape(shapes, shape, html_path)
            elif kind == 'text':
                _add_text_shape(shapes, shape)
            elif kind == 'table':
                _add_table_shape(shapes, shape)
            elif kind == 'autoshape':
                _add_autoshape(shapes, shape)
            elif kind == 'line':
                _add_line_shape(shapes, shape)
            elif kind == 'video':
                _add_video_shape(shapes, shape, html_path)

        # speaker notes
        notes = model.get('notes')
        if notes:
            try:
                slide.notes_slide.notes_text_frame.text = notes
            except Exception:
                pass

    os.makedirs(os.path.dirname(os.path.abspath(output_path)), exist_ok=True)
    prs.save(output_path)
    return prs
