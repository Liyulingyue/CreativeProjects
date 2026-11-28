//! 往返稳定性测试：构造 manifest → 写 PPTX → 读回 → 断言一致。
use std::path::PathBuf;
use phb::model::*;
use phb::ooxml::{read::read_pptx, write::write_pptx};

fn tmp_dir(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("phb_test_{}_{}", std::process::id(), tag));
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn full_theme() -> ThemeManifest {
    ThemeManifest {
        name: Some("路演主题".into()),
        colors: ThemeColors {
            dk1: Some("0A1F44".into()),
            lt1: Some("FFFFFF".into()),
            dk2: Some("071733".into()),
            lt2: Some("EAF1FE".into()),
            accent1: Some("0066F2".into()),
            accent2: Some("12B886".into()),
            accent3: Some("F59F00".into()),
            accent4: Some("E8590C".into()),
            accent5: Some("7048E8".into()),
            accent6: Some("1098AD".into()),
            hlink: Some("0066F2".into()),
            fol_hlink: Some("845EF7".into()),
        },
        major_font: Some("等线 Light".into()),
        minor_font: Some("微软雅黑".into()),
    }
}

fn sample_without_image() -> Presentation {
    Presentation {
        kind: "presentation".into(),
        width_emu: 12192000,
        height_emu: 6858000,
        theme: Some(full_theme()),
        template: None,
        slides: vec![
            // 第 1 页：文字 + 形状 + 线 + 表格
            Slide {
                elements: vec![
                    Element::Text(TextElement {
                        x: 500000, y: 300000, w: 5000000, h: 800000,
                        paragraphs: vec![
                            Paragraph {
                                runs: vec![TextRun {
                                    text: "中文标题测试".into(),
                                    bold: Some(true),
                                    size_pt: Some(32.0),
                                    color: Some("0066F2".into()),
                                    font: Some("微软雅黑".into()),
                                    kern: Some(1200),
                                    spc: Some(200),
                                    lang: Some("en-US".into()),
                                    font_pitch_family: Some(34),
                                    font_charset: Some(0),
                                    font_charset_ea: Some(-122),
                                    font_cs: Some("微软雅黑".into()),
                                    ..Default::default()
                                }],
                                align: Some("center".into()),
                                line_spacing: Some(1.15),
                                ..Default::default()
                            },
                            // 换行段（a:br）+ 带链接的 run + 项目符号 + 缩进/段距
                            Paragraph {
                                runs: vec![
                                    TextRun { text: "第一行".into(), ..Default::default() },
                                    TextRun { text: "\n".into(), bold: Some(true), ..Default::default() },
                                    TextRun {
                                        text: "点这里".into(),
                                        underline: Some(true),
                                        link: Some(Link::url("https://example.com/a?b=1&c=2")),
                                        ..Default::default()
                                    },
                                    TextRun {
                                        text: "跳第2页".into(),
                                        link: Some(Link::slide(2)),
                                        ..Default::default()
                                    },
                                ],
                                bullet: Some(Bullet {
                                    kind: "char".into(),
                                    character: Some("▸".into()),
                                    color: Some("F59F00".into()),
                                    size_pct: Some(90.0),
                                    ..Default::default()
                                }),
                                mar_l: Some(342900),
                                indent: Some(-342900),
                                space_before_pt: Some(6.0),
                                space_after_pt: Some(12.0),
                                ..Default::default()
                            },
                        ],
                        shadow: Some(Shadow {
                            blur_pt: 9.0, dist_pt: 2.0, dir_deg: 45.0,
                            color: "0A1F44".into(), alpha_pct: Some(14.0),
                            algn: Some("bl".into()),
                        }),
                        word_wrap: Some(true),
                        auto_size: Some("text".into()),
                        anchor: None,
                        rotation: None,
                        insets: Some([0, 0, 0, 0]),
                        font_scale_pct: Some(92.5),
                        ln_spc_reduction_pct: Some(10.0),
                    }),
                    Element::Shape(ShapeElement {
                        x: 600000, y: 1400000, w: 3000000, h: 1200000,
                        geometry: "roundRect".into(),
                        fill_type: Some("gradient".into()),
                        fill_stops: Some(vec![
                            GradientStop { pos_pct: 0.0, color: "0066F2".into(), alpha_pct: Some(100.0) },
                            GradientStop { pos_pct: 100.0, color: "7048E8".into(), alpha_pct: None },
                        ]),
                        fill_angle_deg: Some(45.0),
                        line_color: Some("0066F2".into()),
                        line_width_pt: Some(1.5),
                        line_dash: None,
                        rotation: None,
                        shadow: None,
                        paragraphs: Some(vec![Paragraph {
                            runs: vec![TextRun { text: "渐变形状".into(), ..Default::default() }],
                            ..Default::default()
                        }]),
                        anchor: Some("bottom".into()),
                        ..Default::default()
                    }),
                    Element::Line(LineElement {
                        x: 600000, y: 3000000, w: 3000000, h: 200000,
                        color: Some("0066F2".into()),
                        width_pt: Some(1.5),
                        dash: Some("solid".into()),
                        flip_h: false,
                        flip_v: false,
                        head_end: Some("oval".into()),
                        tail_end: Some("triangle".into()),
                    }),
                    Element::Table(TableElement {
                        x: 600000, y: 3500000, w: 3000000, h: 900000,
                        col_widths: vec![1500000, 1500000],
                        row_heights: vec![450000, 450000],
                        cells: vec![
                            vec![
                                TableCell { text: "合并表头".into(), col_span: Some(2), fill_color: Some("EAF1FE".into()), align: Some("center".into()), ..Default::default() },
                                TableCell { text: String::new(), merged_away: true, ..Default::default() },
                            ],
                            vec![
                                TableCell { text: "甲".into(), bold: Some(true), color: Some("0A1F44".into()), size_pt: Some(14.0), ..Default::default() },
                                TableCell { text: "乙\n两行".into(), ..Default::default() },
                            ],
                        ],
                    }),
                ],
                background_color: Some("F7FAFF".into()),
                background_image: None,
                notes: Some("第一页的备注\n第二行备注".into()),
                layout: None,
            },
            // 第 2 页：自由形状 + 自动编号 + 无符号段 + unsupported 标记
            Slide {
                elements: vec![
                    Element::Freeform(FreeformElement {
                        x: 1000000, y: 1000000, w: 2000000, h: 1000000,
                        path_w: Some(2000),
                        path_h: Some(1000),
                        subpaths: vec![
                            SubPath {
                                w: Some(2000),
                                h: Some(1000),
                                closed: true,
                                cmds: vec![
                                    PathCmd::Move { x: 0, y: 500 },
                                    PathCmd::Cubic { x1: 500, y1: 0, x2: 1500, y2: 0, x: 2000, y: 500 },
                                    PathCmd::Arc { w_r: 200, h_r: 200, st_ang_deg: 0.0, sw_ang_deg: 90.0 },
                                    PathCmd::Line { x: 0, y: 500 },
                                ],
                            },
                            SubPath {
                                w: Some(2000),
                                h: Some(1000),
                                closed: false,
                                cmds: vec![PathCmd::Quad { x1: 800, y1: 200, x: 1000, y: 500 }],
                            },
                        ],
                        fill_type: Some("solid".into()),
                        fill_color: Some("12B886".into()),
                        line_color: Some("0A1F44".into()),
                        line_width_pt: Some(2.0),
                        rotation: Some(15.0),
                    }),
                    Element::Text(TextElement {
                        x: 1000000, y: 2500000, w: 3000000, h: 1200000,
                        paragraphs: vec![
                            Paragraph {
                                runs: vec![TextRun { text: "编号项一".into(), ..Default::default() }],
                                bullet: Some(Bullet { kind: "auto_num".into(), num_type: Some("arabicPeriod".into()), num_start: Some(1), ..Default::default() }),
                                level: Some(0),
                                ..Default::default()
                            },
                            Paragraph {
                                runs: vec![TextRun { text: "编号项二".into(), ..Default::default() }],
                                bullet: Some(Bullet { kind: "auto_num".into(), num_type: Some("arabicPeriod".into()), num_start: Some(1), ..Default::default() }),
                                level: Some(1),
                                ..Default::default()
                            },
                            Paragraph {
                                runs: vec![TextRun { text: "无符号段".into(), ..Default::default() }],
                                bullet: Some(Bullet::none()),
                                ..Default::default()
                            },
                        ],
                        anchor: Some("middle".into()),
                        word_wrap: Some(false),
                        auto_size: Some("text".into()),
                        ..Default::default()
                    }),
                    // unsupported 标记：往返必须原样还原
                    Element::Unsupported(UnsupportedElement {
                        x: 6000000, y: 1000000, w: 3000000, h: 2000000,
                        what: "chart".into(),
                        note: "堆积柱状图".into(),
                    }),
                ],
                background_color: None,
                background_image: None,
                notes: None,
                layout: None,
            },
        ],
    }
}

fn sample_image_presentation(png_name: &str) -> Presentation {
    Presentation {
        kind: "presentation".into(),
        width_emu: 12192000,
        height_emu: 6858000,
        theme: Some(full_theme()),
        template: None,
        slides: vec![Slide {
            elements: vec![Element::Image(ImageElement {
                x: 100000, y: 100000, w: 1000000, h: 1000000,
                asset: png_name.into(), // 写入时按内容哈希改名
                rotation: None,
                crop_pct: Some([5.0, 0.0, 10.0, 2.5]),
                link: Some(Link::slide(1)),
            })],
            background_color: None,
            background_image: None,
            notes: None,
            layout: None,
        }],
    }
}

fn write_read(pres: &Presentation, dir: &PathBuf) -> phb::ooxml::read::ReadOutcome {
    write_pptx(pres, dir, &dir.join("out.pptx")).unwrap();
    read_pptx(&dir.join("out.pptx")).unwrap()
}

#[test]
fn roundtrip_without_image_is_fully_stable() {
    let dir = tmp_dir("no_image");
    let pres = sample_without_image();
    let out = write_read(&pres, &dir);
    let a = serde_json::to_string_pretty(&pres).unwrap();
    let b = serde_json::to_string_pretty(&out.manifest).unwrap();
    assert_eq!(a, b, "往返后 manifest 必须逐字节一致");
    // 第二轮也应稳定
    let out2 = write_read(&out.manifest, &dir);
    let c = serde_json::to_string_pretty(&out2.manifest).unwrap();
    assert_eq!(b, c, "第二轮必须与第一轮一致");
}

#[test]
fn roundtrip_image_uses_content_hash_asset_name() {
    let dir = tmp_dir("with_image");
    let pres = sample_image_presentation("anything.png");
    // 1x1 PNG
    let png: Vec<u8> = vec![
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52,
        0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x02, 0x00, 0x00, 0x00, 0x90, 0x77, 0x53,
        0xDE, 0x00, 0x00, 0x00, 0x0C, 0x49, 0x44, 0x41, 0x54, 0x08, 0xD7, 0x63, 0xF8, 0xCF, 0xC0, 0x00,
        0x00, 0x03, 0x01, 0x01, 0x00, 0x18, 0xDD, 0x8D, 0xB0, 0x33, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45,
        0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
    ];
    std::fs::write(dir.join("anything.png"), &png).unwrap();
    write_pptx(&pres, &dir, &dir.join("out.pptx")).unwrap();
    let out = read_pptx(&dir.join("out.pptx")).unwrap();
    if let Element::Image(img) = &out.manifest.slides[0].elements[0] {
        assert!(img.asset.starts_with("img_"), "asset 应为内容哈希命名：{}", img.asset);
        assert!(img.asset.ends_with(".png"));
        // 嵌入的媒体内容应与原文件一致
        let embedded = out.assets.iter().find(|(n, _)| n == &img.asset).expect("asset 应被提取");
        assert_eq!(embedded.1, png, "嵌入字节必须与原始 PNG 一致");
        // 裁切与跳页链接
        assert_eq!(img.crop_pct, Some([5.0, 0.0, 10.0, 2.5]));
        assert_eq!(img.link, Some(Link::slide(1)));
    } else {
        panic!("第一个元素应为 image");
    }
}

#[test]
fn unsupported_survives_roundtrip_invisibly() {
    let dir = tmp_dir("marker");
    let pres = sample_without_image();
    let out = write_read(&pres, &dir);
    // 标记形状在 PPTX 里不可见（noFill/noLine），读回还原为 unsupported
    let found = out.manifest.slides[1]
        .elements
        .iter()
        .any(|e| matches!(e, Element::Unsupported(u) if u.what == "chart" && u.note == "堆积柱状图"));
    assert!(found, "unsupported 标记必须原样还原");
    // XML 里不能有可见内容
    use std::io::Read as _;
    let mut z = zip::ZipArchive::new(std::fs::File::open(dir.join("out.pptx")).unwrap()).unwrap();
    let mut s = String::new();
    z.by_name("ppt/slides/slide2.xml").unwrap().read_to_string(&mut s).unwrap();
    assert!(s.contains("phb:unsupported:chart"));
    assert!(!s.contains("[未支持"), "标记形状不应带可见文字");
}

#[test]
fn notes_and_theme_survive_roundtrip() {
    let dir = tmp_dir("notes_theme");
    let pres = sample_without_image();
    let out = write_read(&pres, &dir);
    assert_eq!(out.manifest.slides[0].notes.as_deref(), Some("第一页的备注\n第二行备注"));
    assert_eq!(out.manifest.slides[1].notes, None);
    let theme = out.manifest.theme.as_ref().expect("主题应被回读");
    assert_eq!(theme.colors.accent1.as_deref(), Some("0066F2"));
    assert_eq!(theme.colors.fol_hlink.as_deref(), Some("845EF7"));
    assert_eq!(theme.minor_font.as_deref(), Some("微软雅黑"));
}

#[test]
fn unknown_field_error_is_helpful() {
    let json = serde_json::json!({
        "kind": "text", "x": 0, "y": 0, "w": 0, "h": 0,
        "colour": "FF0000"
    });
    let err = serde_json::from_value::<Element>(json).unwrap_err().to_string();
    assert!(err.contains("colour"), "报错应指出错误字段：{err}");
    assert!(err.contains("color"), "报错应给出修正建议：{err}");
}

#[test]
fn table_cells_accept_legacy_strings() {
    let json = serde_json::json!({
        "kind": "table", "x": 0, "y": 0, "w": 100, "h": 100,
        "cells": [["甲", {"text": "乙", "col_span": 2}]]
    });
    let el: Element = serde_json::from_value(json).unwrap();
    if let Element::Table(tb) = el {
        assert_eq!(tb.cells[0][0], TableCell::from("甲"));
        assert_eq!(tb.cells[0][1].col_span, Some(2));
    } else {
        panic!("应为 table");
    }
}

#[test]
fn vertical_merge_roundtrip_keeps_grid_positions() {
    let dir = tmp_dir("vmerge");
    let pres = Presentation {
        kind: "presentation".into(),
        width_emu: 12192000,
        height_emu: 6858000,
        theme: Some(full_theme()),
        template: None,
        slides: vec![Slide {
            elements: vec![Element::Table(TableElement {
                x: 0, y: 0, w: 3000000, h: 1200000,
                col_widths: vec![1000000, 1000000, 1000000],
                row_heights: vec![400000, 400000],
                cells: vec![
                    vec![
                        TableCell { text: "跨两行".into(), row_span: Some(2), fill_color: Some("D0E3FF".into()), ..Default::default() },
                        TableCell { text: "A".into(), ..Default::default() },
                        TableCell { text: "B".into(), ..Default::default() },
                    ],
                    vec![
                        TableCell { text: String::new(), merged_away: true, ..Default::default() },
                        TableCell { text: "C".into(), ..Default::default() },
                        TableCell { text: "D".into(), ..Default::default() },
                    ],
                ],
            })],
            background_color: None,
            background_image: None,
            notes: None,
            layout: None,
        }],
    };
    let out = write_read(&pres, &dir);
    let a = serde_json::to_string_pretty(&pres).unwrap();
    let b = serde_json::to_string_pretty(&out.manifest).unwrap();
    assert_eq!(a, b, "纵向合并往返必须逐字节一致");
    // 第二轮同样稳定
    let out2 = write_read(&out.manifest, &dir);
    assert_eq!(b, serde_json::to_string_pretty(&out2.manifest).unwrap());
}
