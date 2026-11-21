//! manifest → HTML 视图（纯渲染，只给人看，永不被解析回读）。
use crate::model::*;
use crate::xmlutil::html_escape;
use std::path::Path;

pub fn view_html(manifest: &Presentation, assets: &[(String, Vec<u8>)], outdir: &Path) -> Result<(), std::io::Error> {
    let assets_dir = outdir.join("assets");
    std::fs::create_dir_all(&assets_dir)?;
    for (name, bytes) in assets {
        std::fs::write(assets_dir.join(name), bytes)?;
    }
    std::fs::create_dir_all(outdir)?;

    let mut index = String::from(
        r#"<!DOCTYPE html><html lang="zh-CN"><head><meta charset="utf-8"><title>deck 预览</title><style>
body{background:#1a2233;font-family:"微软雅黑",sans-serif;margin:0;padding:24px}
.slide-wrap{margin:0 auto 28px;}
.slide-label{color:#8fb4f0;font-size:13px;margin:0 0 6px 2px}
.slide{position:relative;background:#fff;box-shadow:0 6px 24px rgba(0,0,0,.4);overflow:hidden}
</style></head><body>"#,
    );

    let px = |emu: i64| (emu as f64 / 9525.0).round();
    let page_w = px(manifest.width_emu);
    let page_h = px(manifest.height_emu);

    for (i, slide) in manifest.slides.iter().enumerate() {
        index.push_str(&format!(
            r#"<div class="slide-wrap"><p class="slide-label">第 {} 页</p><div class="slide" style="width:{}px;height:{}px">"#,
            i + 1,
            page_w,
            page_h
        ));
        if let Some(bg) = &slide.background_color {
            index.push_str(&format!(r#"<div style="position:absolute;inset:0;background:#{};z-index:0"></div>"#, bg));
        }
        for (zi, el) in slide.elements.iter().enumerate() {
            let (x, y, w, h) = (
                px(pos(el, 'x')),
                px(pos(el, 'y')),
                px(pos(el, 'w')),
                px(pos(el, 'h')),
            );
            let z = zi + 1;
            match el {
                Element::Text(t) => {
                    let mut style = format!(
                        "position:absolute;left:{}px;top:{}px;width:{}px;height:{}px;z-index:{};overflow:hidden",
                        x, y, w, h, z
                    );
                    if let Some(a) = &t.anchor {
                        let v = match a.as_str() {
                            "top" => "flex-start",
                            "middle" => "center",
                            "bottom" => "flex-end",
                            _ => "flex-start",
                        };
                        style.push_str(&format!(";display:flex;flex-direction:column;justify-content:{}", v));
                    }
                    index.push_str(&format!(r#"<div style="{}">"#, style));
                    for para in &t.paragraphs {
                        let align = para.align.as_deref().unwrap_or("left");
                        let ls = para
                            .line_spacing
                            .map(|v| format!("line-height:{};", v))
                            .unwrap_or_default();
                        index.push_str(&format!(r#"<p style="margin:0;text-align:{};{}">"#, align, ls));
                        for run in &para.runs {
                            let mut rs = String::new();
                            if let Some(sz) = run.size_pt {
                                rs.push_str(&format!("font-size:{}pt;", sz));
                            }
                            if run.bold == Some(true) {
                                rs.push_str("font-weight:bold;");
                            }
                            if run.italic == Some(true) {
                                rs.push_str("font-style:italic;");
                            }
                            if let Some(c) = &run.color {
                                rs.push_str(&format!("color:#{};", c));
                            }
                            if let Some(f) = &run.font {
                                rs.push_str(&format!("font-family:'{}',sans-serif;", f));
                            }
                            if run.underline == Some(true) {
                                rs.push_str("text-decoration:underline;");
                            }
                            index.push_str(&format!(r#"<span style="{}">{}</span>"#, rs, html_escape(&run.text)));
                        }
                        index.push_str("</p>");
                    }
                    index.push_str("</div>");
                }
                Element::Shape(sh) => {
                    let mut style = format!(
                        "position:absolute;left:{}px;top:{}px;width:{}px;height:{}px;z-index:{}",
                        x, y, w, h, z
                    );
                    if let Some(c) = &sh.fill_color {
                        if sh.fill_type.as_deref() != Some("none") {
                            style.push_str(&format!(";background:#{}", c));
                        }
                    }
                    if let Some(c) = &sh.line_color {
                        let wpt = sh.line_width_pt.unwrap_or(1.0);
                        style.push_str(&format!(";border:{}pt solid #{}", wpt, c));
                    }
                    if let Some(r) = sh.rotation {
                        style.push_str(&format!(";transform:rotate({}deg)", r));
                    }
                    let radius = if sh.geometry.contains("round") { "border-radius:6px;" } else { "" };
                    index.push_str(&format!(r#"<div style="{}{}"></div>"#, style, radius));
                    if let Some(ps) = &sh.paragraphs {
                        index.push_str(&format!(r#"<div style="position:absolute;left:{}px;top:{}px;width:{}px;height:{}px;z-index:{};overflow:hidden;display:flex;flex-direction:column;justify-content:center">"#, x, y, w, h, z + 1));
                        for para in ps {
                            index.push_str(r#"<p style="margin:0;text-align:center">"#);
                            for run in &para.runs {
                                let mut rs = String::new();
                                if let Some(sz) = run.size_pt {
                                    rs.push_str(&format!("font-size:{}pt;", sz));
                                }
                                if run.bold == Some(true) {
                                    rs.push_str("font-weight:bold;");
                                }
                                if let Some(c) = &run.color {
                                    rs.push_str(&format!("color:#{};", c));
                                }
                                index.push_str(&format!(r#"<span style="{}">{}</span>"#, rs, html_escape(&run.text)));
                            }
                            index.push_str("</p>");
                        }
                        index.push_str("</div>");
                    }
                }
                Element::Image(img) => {
                    let asset_url = format!("assets/{}", html_escape(&img.asset));
                    index.push_str(&format!(
                        r#"<img src="{}" style="position:absolute;left:{}px;top:{}px;width:{}px;height:{}px;z-index:{}">"#,
                        asset_url, x, y, w, h, z
                    ));
                }
                Element::Line(l) => {
                    // 用一条细 div 近似（对角线不再画斜线，仅示意起止范围）
                    let (lx, ly, lw) = if l.w == 0 {
                        (x, y, 2.0)
                    } else {
                        (x, y + h / 2.0, w as f64)
                    };
                    let mut style = format!(
                        "position:absolute;left:{}px;top:{}px;width:{}px;height:2px;z-index:{};overflow:visible",
                        lx, ly, lw, z
                    );
                    if let Some(c) = &l.color {
                        style.push_str(&format!(";background:#{}", c));
                    }
                    index.push_str(&format!(r#"<div style="{}"></div>"#, style));
                }
                Element::Table(tb) => {
                    let mut style = format!(
                        "position:absolute;left:{}px;top:{}px;z-index:{};border-collapse:collapse",
                        x, y, z
                    );
                    let _ = &mut style;
                    let mut html = String::from("<table style=\"");
                    html.push_str(&style);
                    html.push_str("\" border=\"0\" cellspacing=\"0\" cellpadding=\"6\">");
                    for row in &tb.cells {
                        html.push_str("<tr>");
                        for cell in row {
                            html.push_str(&format!(
                                r#"<td style="border:1px solid #c9d4e8;background:#fff">{}</td>"#,
                                html_escape(cell)
                            ));
                        }
                        html.push_str("</tr>");
                    }
                    html.push_str("</table>");
                    index.push_str(&html);
                }
                Element::Unsupported(u) => {
                    index.push_str(&format!(
                        r#"<div style="position:absolute;left:{}px;top:{}px;width:{}px;height:{}px;z-index:{};outline:2px dashed #9AA7B8;display:flex;align-items:center;justify-content:center;color:#6B7A90;font-size:11px">[未支持: {}]</div>"#,
                        x, y, w, h, z, html_escape(&u.what)
                    ));
                }
                _ => {}
            }
        }
        index.push_str("</div></div>");
    }

    index.push_str("</body></html>");
    std::fs::write(outdir.join("index.html"), index)?;
    Ok(())
}

fn pos(el: &Element, axis: char) -> i64 {
    match el {
        Element::Text(t) => match axis {
            'x' => t.x,
            'y' => t.y,
            'w' => t.w,
            _ => t.h,
        },
        Element::Shape(s) => match axis {
            'x' => s.x,
            'y' => s.y,
            'w' => s.w,
            _ => s.h,
        },
        Element::Image(i) => match axis {
            'x' => i.x,
            'y' => i.y,
            'w' => i.w,
            _ => i.h,
        },
        Element::Line(l) => match axis {
            'x' => l.x,
            'y' => l.y,
            'w' => l.w,
            _ => l.h,
        },
        Element::Table(t) => match axis {
            'x' => t.x,
            'y' => t.y,
            'w' => t.w,
            _ => t.h,
        },
        Element::Unsupported(u) => match axis {
            'x' => u.x,
            'y' => u.y,
            'w' => u.w,
            _ => u.h,
        },
        _ => 0,
    }
}


