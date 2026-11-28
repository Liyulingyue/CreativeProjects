//! manifest → HTML 视图（纯渲染，只给人看，永不被解析回读）。
//! v0.2：SVG 自由形状、线性渐变、超链接、项目符号、合并表格、备注面板、背景图。
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
.notes{margin:8px auto 0;max-width:960px;color:#c7d5ee;font-size:13px;line-height:1.6;background:#232e47;border-left:3px solid #0066F2;padding:8px 14px;border-radius:0 6px 6px 0}
a{color:inherit}
</style></head><body>"#,
    );

    let px = |emu: i64| (emu as f64 / 9525.0).round();
    let page_w = px(manifest.width_emu);
    let page_h = px(manifest.height_emu);

    for (i, slide) in manifest.slides.iter().enumerate() {
        index.push_str(&format!(
            r#"<div class="slide-wrap" id="slide-{}"><p class="slide-label">第 {} 页</p><div class="slide" style="width:{}px;height:{}px">"#,
            i + 1,
            i + 1,
            page_w,
            page_h
        ));
        // 背景：图片优先
        if let Some(bg_img) = &slide.background_image {
            index.push_str(&format!(
                r#"<div style="position:absolute;inset:0;z-index:0;background:url('assets/{}') center/cover"></div>"#,
                html_escape(bg_img)
            ));
        } else if let Some(bg) = &slide.background_color {
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
                    render_paragraphs(&mut index, &t.paragraphs);
                    index.push_str("</div>");
                }
                Element::Shape(sh) => {
                    let mut style = format!(
                        "position:absolute;left:{}px;top:{}px;width:{}px;height:{}px;z-index:{}",
                        x, y, w, h, z
                    );
                    match sh.fill_type.as_deref() {
                        Some("none") => {}
                        Some("gradient") => {
                            style.push_str(&format!(";background:{}", css_gradient(sh.fill_stops.as_deref(), sh.fill_angle_deg)));
                        }
                        _ => {
                            if let Some(c) = &sh.fill_color {
                                style.push_str(&format!(";background:{}", css_color(c, sh.fill_alpha_pct)));
                            }
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
                        render_paragraphs(&mut index, ps);
                        index.push_str("</div>");
                    }
                }
                Element::Image(img) => {
                    let asset_url = format!("assets/{}", html_escape(&img.asset));
                    let mut img_tag = format!(
                        r#"<img src="{}" style="position:absolute;left:{}px;top:{}px;width:{}px;height:{}px;z-index:{}"#,
                        asset_url, x, y, w, h, z
                    );
                    if let Some(r) = img.rotation {
                        img_tag.push_str(&format!(";transform:rotate({}deg)", r));
                    }
                    img_tag.push_str("\">");
                    if let Some(link) = &img.link {
                        let href = css_href(link);
                        let tip = link
                            .tooltip
                            .as_ref()
                            .map(|t| format!(r#" title="{}""#, html_escape(t)))
                            .unwrap_or_default();
                        index.push_str(&format!(
                            r#"<a href="{}"{} style="position:absolute;left:{}px;top:{}px;width:{}px;height:{}px;z-index:{}">{}</a>"#,
                            href, tip, x, y, w, h, z, img_tag
                        ));
                    } else {
                        index.push_str(&img_tag);
                    }
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
                    let mut arrow = String::new();
                    if l.tail_end.as_deref().map(|t| t != "none").unwrap_or(false) {
                        arrow.push_str("▶");
                    }
                    index.push_str(&format!(r#"<div style="{}">{}</div>"#, style, arrow));
                }
                Element::Table(tb) => {
                    let mut html = String::from(&format!(
                        r#"<table style="position:absolute;left:{}px;top:{}px;z-index:{};border-collapse:collapse""#,
                        x, y, z
                    ));
                    html.push_str("\" border=\"0\" cellspacing=\"0\" cellpadding=\"6\">");
                    for row in &tb.cells {
                        html.push_str("<tr>");
                        for cell in row {
                            if cell.merged_away {
                                continue;
                            }
                            let mut attrs = String::new();
                            if let Some(cs) = cell.col_span {
                                attrs.push_str(&format!(" colspan=\"{}\"", cs));
                            }
                            if let Some(rs) = cell.row_span {
                                attrs.push_str(&format!(" rowspan=\"{}\"", rs));
                            }
                            let mut st = String::from("border:1px solid #c9d4e8;");
                            if let Some(c) = &cell.fill_color {
                                st.push_str(&format!("background:#{};", c));
                            }
                            if let Some(b) = cell.bold {
                                if b {
                                    st.push_str("font-weight:bold;");
                                }
                            }
                            if let Some(sz) = cell.size_pt {
                                st.push_str(&format!("font-size:{}pt;", sz));
                            }
                            if let Some(c) = &cell.color {
                                st.push_str(&format!("color:#{};", c));
                            }
                            if let Some(a) = &cell.align {
                                st.push_str(&format!("text-align:{};", a));
                            }
                            let text = html_escape(&cell.text).replace('\n', "<br>");
                            html.push_str(&format!(r#"<td{} style="{}">{}</td>"#, attrs, st, text));
                        }
                        html.push_str("</tr>");
                    }
                    html.push_str("</table>");
                    index.push_str(&html);
                }
                Element::Freeform(f) => {
                    let pw = f.path_w.unwrap_or_else(|| f.w.max(1));
                    let ph = f.path_h.unwrap_or_else(|| f.h.max(1));
                    let mut d = String::new();
                    for sp in &f.subpaths {
                        for cmd in &sp.cmds {
                            match cmd {
                                PathCmd::Move { x, y } => d.push_str(&format!("M {} {} ", x, y)),
                                PathCmd::Line { x, y } => d.push_str(&format!("L {} {} ", x, y)),
                                PathCmd::Cubic { x1, y1, x2, y2, x, y } => {
                                    d.push_str(&format!("C {} {} {} {} {} {} ", x1, y1, x2, y2, x, y))
                                }
                                PathCmd::Quad { x1, y1, x, y } => d.push_str(&format!("Q {} {} {} {} ", x1, y1, x, y)),
                                PathCmd::Arc { .. } => {} // HTML 预览近似略过弧
                            }
                        }
                        if sp.closed {
                            d.push_str("Z ");
                        }
                    }
                    let fill = match f.fill_type.as_deref() {
                        Some("none") => "none".to_string(),
                        _ => f
                            .fill_color
                            .as_ref()
                            .map(|c| format!("#{}", c))
                            .unwrap_or_else(|| "none".to_string()),
                    };
                    let stroke = f
                        .line_color
                        .as_ref()
                        .map(|c| format!("#{}", c))
                        .unwrap_or_else(|| "none".to_string());
                    let stroke_w = f.line_width_pt.unwrap_or(1.0);
                    let mut path_attr = format!(
                        r#"d="{}" fill="{}" stroke="{}" stroke-width="{}""#,
                        html_escape(d.trim()),
                        fill,
                        stroke,
                        stroke_w
                    );
                    if let Some(r) = f.rotation {
                        path_attr = format!(
                            r#"transform="rotate({} {} {})" {}"#,
                            r,
                            pw as f64 / 2.0,
                            ph as f64 / 2.0,
                            path_attr
                        );
                    }
                    index.push_str(&format!(
                        r#"<svg style="position:absolute;left:{}px;top:{}px;width:{}px;height:{}px;z-index:{};overflow:visible" viewBox="0 0 {pw} {ph}" preserveAspectRatio="none"><path {}/></svg>"#,
                        x, y, w, h, z, path_attr
                    ));
                }
                Element::Unsupported(u) => {
                    index.push_str(&format!(
                        r#"<div style="position:absolute;left:{}px;top:{}px;width:{}px;height:{}px;z-index:{};outline:2px dashed #9AA7B8;display:flex;align-items:center;justify-content:center;color:#6B7A90;font-size:11px">[未支持: {}]</div>"#,
                        x, y, w, h, z, html_escape(&u.what)
                    ));
                }
            }
        }
        index.push_str("</div>");
        if let Some(notes) = &slide.notes {
            let html_notes = html_escape(notes).replace('\n', "<br>");
            index.push_str(&format!(r#"<div class="notes">📝 备注：{}</div>"#, html_notes));
        }
        index.push_str("</div>");
    }

    index.push_str("</body></html>");
    std::fs::write(outdir.join("index.html"), index)?;
    Ok(())
}

/// 段落序列（含项目符号 / 缩进 / 行距 / 链接 / 换行）
fn render_paragraphs(out: &mut String, paragraphs: &[Paragraph]) {
    let mut counters: [u64; 9] = [0; 9]; // 自动编号计数（按 level）
    for para in paragraphs {
        let align = para.align.as_deref().unwrap_or("left");
        let ls = para
            .line_spacing
            .map(|v| format!("line-height:{};", v))
            .unwrap_or_default();
        let lvl = para.level.unwrap_or(0).clamp(0, 8) as usize;
        let mut pad = para
            .mar_l
            .map(|v| format!("margin-left:{}px;", (v as f64 / 9525.0).round()))
            .unwrap_or_default();
        if para.indent.is_some() {
            pad.push_str(&format!(
                "text-indent:{}px;",
                (para.indent.unwrap() as f64 / 9525.0).round()
            ));
        }
        if let Some(sb) = para.space_before_pt {
            pad.push_str(&format!("margin-top:{}pt;", sb));
        }
        if let Some(sa) = para.space_after_pt {
            pad.push_str(&format!("margin-bottom:{}pt;", sa));
        }
        let marker = match &para.bullet {
            Some(b) if b.kind == "char" => {
                let ch = b.character.clone().unwrap_or_else(|| "•".into());
                let color = b
                    .color
                    .as_ref()
                    .map(|c| format!(";color:#{}", c))
                    .unwrap_or_default();
                let sz = b
                    .size_pct
                    .map(|s| format!(";font-size:{}%", s))
                    .unwrap_or_default();
                Some(format!("<span style=\"{}{}\">{} </span>", color, sz, html_escape(&ch)))
            }
            Some(b) if b.kind == "auto_num" => {
                counters[lvl] += 1;
                let n = b.num_start.map(|s| s as u64 + counters[lvl]).unwrap_or(counters[lvl]);
                Some(format!("<span>{}. </span>", n))
            }
            Some(_) => Some(String::new()), // none：不出符号
            None => None,
        };
        out.push_str(&format!(r#"<p style="margin:0;text-align:{};{}">"#, align, ls + &pad));
        if let Some(m) = &marker {
            out.push_str(m);
        }
        for run in &para.runs {
            let (before, after) = match &run.link {
                Some(l) => {
                    let tip = l
                        .tooltip
                        .as_ref()
                        .map(|t| format!(r#" title="{}""#, html_escape(t)))
                        .unwrap_or_default();
                    (format!(r#"<a href="{}"{}>"#, css_href(l), tip), "</a>".to_string())
                }
                None => (String::new(), String::new()),
            };
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
            // 换行段
            if run.text == "\n" {
                out.push_str("<br>");
                continue;
            }
            out.push_str(&format!(
                "{}<span style=\"{}\">{}</span>{}",
                before,
                rs,
                html_escape(&run.text),
                after
            ));
        }
        out.push_str("</p>");
    }
}

fn css_href(link: &Link) -> String {
    if let Some(u) = &link.url {
        html_escape(u)
    } else if let Some(n) = link.slide {
        format!("#slide-{}", n)
    } else {
        "#".to_string()
    }
}

/// RRGGBB(+alpha%) → CSS 颜色
fn css_color(hex: &str, alpha_pct: Option<f64>) -> String {
    match alpha_pct {
        Some(a) => {
            let v = u32::from_str_radix(hex, 16).unwrap_or(0);
            format!("rgba({},{},{},{:.2})", v >> 16, (v >> 8) & 0xFF, v & 0xFF, a / 100.0)
        }
        None => format!("#{}", hex),
    }
}

/// 停靠点 + 角度 → CSS linear-gradient（OOXML 0°=向右，CSS 0°=向上，差 90°）
fn css_gradient(stops: Option<&[GradientStop]>, angle_deg: Option<f64>) -> String {
    let Some(stops) = stops else { return "none".to_string() };
    if stops.is_empty() {
        return "none".to_string();
    }
    let css_deg = angle_deg.unwrap_or(0.0) + 90.0;
    let mut parts = Vec::new();
    for s in stops {
        parts.push(format!("{} {}%", css_color(&s.color, s.alpha_pct), s.pos_pct));
    }
    format!("linear-gradient({}deg, {})", css_deg, parts.join(", "))
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
        Element::Freeform(f) => match axis {
            'x' => f.x,
            'y' => f.y,
            'w' => f.w,
            _ => f.h,
        },
        Element::Unsupported(u) => match axis {
            'x' => u.x,
            'y' => u.y,
            'w' => u.w,
            _ => u.h,
        },
    }
}
