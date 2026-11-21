//! 往返稳定性测试：构造 manifest → 写 PPTX → 读回 → 断言一致。
use std::path::PathBuf;
use phb::model::*;
use phb::ooxml::{read::read_pptx, write::write_pptx};

fn tmp_dir(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("phb_test_{}_{}", std::process::id(), tag));
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn sample_without_image() -> Presentation {
    Presentation {
        kind: "presentation".into(),
        width_emu: 12192000,
        height_emu: 6858000,
        slides: vec![Slide {
            elements: vec![
                Element::Text(TextElement {
                    x: 500000, y: 300000, w: 5000000, h: 800000,
                    paragraphs: vec![Paragraph {
                        runs: vec![TextRun {
                            text: "中文标题测试".into(),
                            bold: Some(true),
                            size_pt: Some(32.0),
                            color: Some("0066F2".into()),
                            font: Some("微软雅黑".into()),
                            ..Default::default()
                        }],
                        align: Some("center".into()),
                        ..Default::default()
                    }],
                    shadow: Some(Shadow {
                        blur_pt: 9.0, dist_pt: 2.0, dir_deg: 45.0,
                        color: "0A1F44".into(), alpha_pct: Some(14.0),
                    }),
                    word_wrap: Some(true),
                    auto_size: None,
                    anchor: None,
                    rotation: None,
                }),
                Element::Shape(ShapeElement {
                    x: 600000, y: 1400000, w: 3000000, h: 1200000,
                    geometry: "roundRect".into(),
                    fill_type: Some("solid".into()),
                    fill_color: Some("EAF1FE".into()),
                    line_color: Some("0066F2".into()),
                    line_width_pt: Some(1.5),
                    line_dash: None,
                    rotation: None,
                    shadow: None,
                    paragraphs: Some(vec![Paragraph {
                        runs: vec![TextRun { text: "形状内文字".into(), ..Default::default() }],
                        ..Default::default()
                    }]),
                }),
                Element::Line(LineElement {
                    x: 600000, y: 3000000, w: 3000000, h: 200000,
                    color: Some("0066F2".into()),
                    width_pt: Some(1.5),
                    dash: Some("solid".into()),
                    flip_h: false, flip_v: false,
                    tail_end: Some("triangle".into()),
                }),
                Element::Table(TableElement {
                    x: 600000, y: 3500000, w: 3000000, h: 900000,
                    col_widths: vec![1500000, 1500000],
                    row_heights: vec![450000, 450000],
                    cells: vec![vec!["甲".into(), "乙".into()], vec!["丙".into(), "丁".into()]],
                }),
            ],
            background_color: None,
            notes: None,
        }],
    }
}

#[test]
fn roundtrip_without_image_is_fully_stable() {
    let dir = tmp_dir("no_image");
    let pres = sample_without_image();
    write_pptx(&pres, &dir, &dir.join("out.pptx")).unwrap();
    let out = read_pptx(&dir.join("out.pptx")).unwrap();
    let a = serde_json::to_string_pretty(&pres).unwrap();
    let b = serde_json::to_string_pretty(&out.manifest).unwrap();
    assert_eq!(a, b, "往返后 manifest 必须逐字节一致");
    // 第二轮也应稳定
    write_pptx(&out.manifest, &dir, &dir.join("out2.pptx")).unwrap();
    let out2 = read_pptx(&dir.join("out2.pptx")).unwrap();
    let c = serde_json::to_string_pretty(&out2.manifest).unwrap();
    assert_eq!(b, c, "第二轮必须与第一轮一致");
}

#[test]
fn roundtrip_image_uses_content_hash_asset_name() {
    let dir = tmp_dir("with_image");
    let pres = Presentation {
        kind: "presentation".into(),
        width_emu: 12192000,
        height_emu: 6858000,
        slides: vec![Slide {
            elements: vec![Element::Image(ImageElement {
                x: 100000, y: 100000, w: 1000000, h: 1000000,
                asset: "anything.png".into(),  // 写入时按内容哈希改名
                rotation: None,
                crop_pct: None,
            })],
            background_color: None,
            notes: None,
        }],
    };
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
    } else {
        panic!("第一个元素应为 image");
    }
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
