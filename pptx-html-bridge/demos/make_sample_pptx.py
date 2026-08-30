#!/usr/bin/env python3
"""
生成演示用的 PPTX 文件：demos/source/test.pptx

使用 python-pptx 构造一个包含多种元素的演示文稿：
- 纯色背景、带标题/正文的文本页
- 自动形状、线条、表格
- 内嵌图片（无需 Pillow，直接写入 PNG 字节）
"""

import os
import base64

from pptx import Presentation
from pptx.util import Inches, Pt, Emu
from pptx.dml.color import RGBColor
from pptx.enum.text import PP_ALIGN
from pptx.enum.shapes import MSO_SHAPE

# 2x2 红色 PNG（1x1 放大即可，python-pptx 不校验内容）
PNG_BYTES = base64.b64decode(
    "iVBORw0KGgoAAAANSUhEUgAAAAIAAAACCAYAAABytg0kAAAAEklEQVR4nGP8z8Dwn4GBgYEA"
    "KicDdwTgXbAAAAAAAElFTkSuQmCC"
)


def build_sample_pptx(path):
    prs = Presentation()
    prs.slide_width = Inches(10)
    prs.slide_height = Inches(7.5)

    # ---- Slide 1: 标题页，纯色深色背景 ----
    slide = prs.slides.add_slide(prs.slide_layouts[6])  # blank
    bg = slide.background.fill
    bg.solid()
    bg.fore_color.rgb = RGBColor(0x1B, 0x2A, 0x4A)  # 深蓝

    title = slide.shapes.add_textbox(Inches(1), Inches(2.5), Inches(8), Inches(1.2))
    p = title.text_frame.paragraphs[0]
    p.alignment = PP_ALIGN.CENTER
    run = p.add_run()
    run.text = "pptx-html-bridge 演示文稿"
    run.font.size = Pt(40)
    run.font.bold = True
    run.font.color.rgb = RGBColor(0xFF, 0xFF, 0xFF)

    sub = slide.shapes.add_textbox(Inches(1), Inches(3.8), Inches(8), Inches(0.8))
    p = sub.text_frame.paragraphs[0]
    p.alignment = PP_ALIGN.CENTER
    run = p.add_run()
    run.text = "PPTX to HTML 转换库 <端到端> 测试"
    run.font.size = Pt(20)
    run.font.color.rgb = RGBColor(0xBF, 0xDB, 0xFE)

    # ---- Slide 2: 形状、线条、图片 ----
    slide = prs.slides.add_slide(prs.slide_layouts[6])
    bg = slide.background.fill
    bg.solid()
    bg.fore_color.rgb = RGBColor(0xFF, 0xFF, 0xFF)

    box = slide.shapes.add_shape(MSO_SHAPE.ROUNDED_RECTANGLE, Inches(0.8), Inches(0.8), Inches(3.5), Inches(2))
    box.fill.solid()
    box.fill.fore_color.rgb = RGBColor(0x25, 0x63, 0xEB)
    box.line.color.rgb = RGBColor(0x1E, 0x40, 0xAF)
    box.line.width = Pt(2)

    line = slide.shapes.add_connector(1, Inches(0.8), Inches(3.4), Inches(9.2), Inches(3.4))  # straight line
    line.line.color.rgb = RGBColor(0x66, 0x66, 0x66)
    line.line.width = Pt(1.5)

    img = slide.shapes.add_picture(__import__("io").BytesIO(PNG_BYTES), Inches(1), Inches(4), width=Inches(2), height=Inches(2))

    cap = slide.shapes.add_textbox(Inches(3.5), Inches(4.5), Inches(5), Inches(1))
    p = cap.text_frame.paragraphs[0]
    run = p.add_run()
    run.text = "上方：形状 / 线条 / 内嵌图片"
    run.font.size = Pt(18)
    run.font.color.rgb = RGBColor(0x33, 0x33, 0x33)

    # ---- Slide 3: 表格 ----
    slide = prs.slides.add_slide(prs.slide_layouts[6])
    rows, cols = 3, 3
    table_shape = slide.shapes.add_table(rows, cols, Inches(1.5), Inches(1.5), Inches(7), Inches(3))
    table = table_shape.table
    data = [
        ["功能", "状态", "备注"],
        ["文本样式", "✓", "字体 / 颜色 / 加粗"],
        ["媒体资源", "✓", "图片 / 视频"],
    ]
    for r, row in enumerate(data):
        for c, val in enumerate(row):
            cell = table.cell(r, c)
            cell.text = val
            for para in cell.text_frame.paragraphs:
                for run in para.runs:
                    run.font.size = Pt(14)

    prs.save(path)
    return path


def main():
    script_dir = os.path.dirname(os.path.abspath(__file__))
    source_dir = os.path.join(script_dir, "source")
    os.makedirs(source_dir, exist_ok=True)
    path = os.path.join(source_dir, "test.pptx")
    build_sample_pptx(path)
    print(f"✓ 已生成演示文件: {path}")
    return 0


if __name__ == "__main__":
    exit(main())
