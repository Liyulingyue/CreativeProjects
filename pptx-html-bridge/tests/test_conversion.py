import os

from pptx_html_bridge import PPTXToHTMLConverter, convert_pptx_to_html


def test_convert_file_creates_structure(sample_pptx, tmp_path):
    output_dir = os.path.join(str(tmp_path), "out")
    result = convert_pptx_to_html(sample_pptx, output_dir)

    assert result["slides_count"] == 5
    assert os.path.isdir(os.path.join(output_dir, "slides"))
    assert os.path.isdir(os.path.join(output_dir, "media"))
    for i in (1, 2, 3, 4, 5):
        assert os.path.isfile(os.path.join(output_dir, "slides", f"slide{i}.html"))
    assert os.path.isfile(os.path.join(output_dir, "sample_index.html"))
    assert len(result["generated_files"]) == 6  # 5 slides + index


def test_slide1_dark_background_and_white_text(sample_pptx, tmp_path):
    output_dir = os.path.join(str(tmp_path), "out")
    convert_pptx_to_html(sample_pptx, output_dir)

    html = open(os.path.join(output_dir, "slides", "slide1.html"), encoding="utf-8").read()
    assert "background-color: #1b2a4a;" in html
    # explicit white text color kept
    assert "color: #ffffff;" in html
    # HTML in text is escaped
    assert "Hello &amp; &lt;World&gt;" in html
    # unstyled subtitle gets readable fallback (dark text on dark bg would be unreadable,
    # but luminance of #1b2a4a is low -> white fallback)
    assert "plain subtitle" in html


def test_slide2_shapes_line_picture(sample_pptx, tmp_path):
    output_dir = os.path.join(str(tmp_path), "out")
    convert_pptx_to_html(sample_pptx, output_dir)

    html = open(os.path.join(output_dir, "slides", "slide2.html"), encoding="utf-8").read()
    assert "background-color: #ffffff;" in html
    assert "auto-shape" in html
    assert "background-color: #2563eb;" in html
    assert "class=\"shape line\"" in html
    # picture extracted to media/ with correct relative path (prefixed by source name)
    assert "../media/sample_slide2_img0.png" in html
    media_files = os.listdir(os.path.join(output_dir, "media"))
    assert "sample_slide2_img0.png" in media_files


def test_slide3_table_escapes_html(sample_pptx, tmp_path):
    output_dir = os.path.join(str(tmp_path), "out")
    convert_pptx_to_html(sample_pptx, output_dir)

    html = open(os.path.join(output_dir, "slides", "slide3.html"), encoding="utf-8").read()
    assert '<table style="table-layout: fixed' in html
    assert "A&amp;B" in html
    assert "&lt;tag&gt;" in html
    assert "<tag>" not in html


def test_navigation_links(sample_pptx, tmp_path):
    output_dir = os.path.join(str(tmp_path), "out")
    convert_pptx_to_html(sample_pptx, output_dir)

    slide2 = open(os.path.join(output_dir, "slides", "slide2.html"), encoding="utf-8").read()
    assert 'href="slide1.html"' in slide2
    assert 'href="slide3.html"' in slide2
    slide1 = open(os.path.join(output_dir, "slides", "slide1.html"), encoding="utf-8").read()
    assert "slide0.html" not in slide1


def test_index_page_links_all_slides(sample_pptx, tmp_path):
    output_dir = os.path.join(str(tmp_path), "out")
    convert_pptx_to_html(sample_pptx, output_dir)

    index = open(os.path.join(output_dir, "sample_index.html"), encoding="utf-8").read()
    for i in (1, 2, 3):
        assert f'slides/slide{i}.html' in index


def test_compact_output(sample_pptx, tmp_path):
    output_dir = os.path.join(str(tmp_path), "out")
    convert_pptx_to_html(sample_pptx, output_dir, compact=True)

    html = open(os.path.join(output_dir, "slides", "slide1.html"), encoding="utf-8").read()
    assert "\n" not in html


def test_missing_file_raises(tmp_path):
    converter = PPTXToHTMLConverter()
    try:
        converter.convert_file(os.path.join(str(tmp_path), "nope.pptx"))
        assert False, "expected FileNotFoundError"
    except FileNotFoundError:
        pass
