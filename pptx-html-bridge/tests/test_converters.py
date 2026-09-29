from pptx.enum.dml import MSO_LINE_DASH_STYLE

from pptx_html_bridge.converters import (
    emu_to_px,
    emu_to_pt,
    color_to_hex,
    pt_to_px,
    dash_style_to_css,
)
from pptx_html_bridge.html_generators import escape_text, pick_default_text_color


def test_emu_to_px():
    # 914400 EMU = 1 inch = 96 px
    assert emu_to_px(914400) == 96
    assert emu_to_px(0) == 0


def test_emu_to_pt():
    # 12700 EMU = 1 pt
    assert emu_to_pt(12700) == 1.0


def test_pt_to_px():
    assert pt_to_px(72) == 96
    assert pt_to_px(18) == 24


def test_color_to_hex():
    class FakeRGB:
        rgb = (0x12, 0x34, 0x56)

    assert color_to_hex(FakeRGB()) == "#123456"
    assert color_to_hex(None) is None


def test_dash_style_to_css():
    assert dash_style_to_css(None) == "solid"
    assert dash_style_to_css(MSO_LINE_DASH_STYLE.SOLID) == "solid"
    assert dash_style_to_css(MSO_LINE_DASH_STYLE.DASH) == "dashed"
    assert dash_style_to_css(MSO_LINE_DASH_STYLE.ROUND_DOT) == "dotted"
    assert dash_style_to_css(MSO_LINE_DASH_STYLE.SQUARE_DOT) == "dotted"
    assert dash_style_to_css(MSO_LINE_DASH_STYLE.LONG_DASH) == "dashed"


def test_escape_text():
    assert escape_text("a<b>&c") == "a&lt;b&gt;&amp;c"
    assert escape_text("") == ""


def test_pick_default_text_color_dark_background():
    style = "background-color: #1b2a4a;"
    assert pick_default_text_color(style) == "#ffffff"


def test_pick_default_text_color_light_background():
    style = "background-color: #ffffff;"
    assert pick_default_text_color(style) == "#1f2937"


def test_pick_default_text_color_unknown():
    assert pick_default_text_color("background-image: url('x.png');") == "#1f2937"
    assert pick_default_text_color("") == "#1f2937"
