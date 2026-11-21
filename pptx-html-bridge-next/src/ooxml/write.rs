//! manifest → PPTX 写入器。用嵌入的空白模板打底，注入 slides / media / rels / content-types。
use super::parts;
use crate::model::*;
use crate::xmlutil::escape;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub fn write_pptx(manifest: &Presentation, assets_dir: &Path, out: &Path) -> Result<(), crate::error::BridgeError> {
    let file = std::fs::File::create(out)?;
    let mut zip = zip::ZipWriter::new(file);
    let options = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);

    // 1. 媒体映射：asset 文件名 → 包内路径
    let mut media_map: HashMap<String, String> = HashMap::new();
    let mut media_parts: Vec<(String, Vec<u8>)> = Vec::new();
    let mut next_media = 1;
    for slide in &manifest.slides {
        for el in &slide.elements {
            if let Element::Image(img) = el {
                if media_map.contains_key(&img.asset) {
                    continue;
                }
                let ext = PathBuf::from(&img.asset)
                    .extension()
                    .and_then(|e| e.to_str())
                    .unwrap_or("png")
                    .to_lowercase();
                let part = format!("media/image{}.{}", next_media, ext);
                next_media += 1;
                media_map.insert(img.asset.clone(), part.clone());
                let bytes = std::fs::read(assets_dir.join(&img.asset)).unwrap_or_default();
                media_parts.push((part, bytes));
            }
        }
    }

    // 2. 逐页 XML + rels
    let mut slide_parts: Vec<(String, String)> = Vec::new();
    let mut slide_rels_parts: Vec<(String, String)> = Vec::new();
    for (i, slide) in manifest.slides.iter().enumerate() {
        let (xml, rels) = emit_slide(slide, &media_map, assets_dir);
        slide_parts.push((format!("ppt/slides/slide{}.xml", i + 1), xml));
        slide_rels_parts.push((format!("ppt/slides/_rels/slide{}.xml.rels", i + 1), rels));
    }

    // 3. 打包
    let ct = patch_content_types(manifest.slides.len(), &media_map);
    let pres_rels = patch_pres_rels(manifest.slides.len());
    let pres = patch_presentation(manifest);

    let w = |zip: &mut zip::ZipWriter<std::fs::File>, name: &str, bytes: &[u8]| -> std::io::Result<()> {
        use std::io::Write;
        zip.start_file(name, options)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
        zip.write_all(bytes)
    };

    w(&mut zip, "[Content_Types].xml", ct.as_bytes())?;
    w(&mut zip, "_rels/.rels", parts::ROOT_RELS.as_bytes())?;
    w(&mut zip, "docProps/core.xml", parts::DOC_CORE.as_bytes())?;
    w(&mut zip, "docProps/app.xml", parts::DOC_APP.as_bytes())?;
    w(&mut zip, "ppt/presentation.xml", pres.as_bytes())?;
    w(&mut zip, "ppt/_rels/presentation.xml.rels", pres_rels.as_bytes())?;
    w(&mut zip, "ppt/slideMasters/slideMaster1.xml", parts::MASTER_XML.as_bytes())?;
    w(&mut zip, "ppt/slideMasters/_rels/slideMaster1.xml.rels", parts::MASTER_RELS.as_bytes())?;
    for (i, xml) in parts::LAYOUT_XML.iter().enumerate() {
        w(&mut zip, &format!("ppt/slideLayouts/slideLayout{}.xml", i + 1), xml.as_bytes())?;
        w(&mut zip, &format!("ppt/slideLayouts/_rels/slideLayout{}.xml.rels", i + 1), parts::LAYOUT_RELS[i].as_bytes())?;
    }
    w(&mut zip, "ppt/theme/theme1.xml", parts::THEME_XML.as_bytes())?;
    w(&mut zip, "ppt/presProps.xml", parts::PRES_PROPS.as_bytes())?;
    w(&mut zip, "ppt/viewProps.xml", parts::VIEW_PROPS.as_bytes())?;
    w(&mut zip, "ppt/tableStyles.xml", parts::TABLE_STYLES.as_bytes())?;
    w(&mut zip, "ppt/printerSettings/printerSettings1.bin", parts::PRINTER_BIN)?;
    w(&mut zip, "docProps/thumbnail.jpeg", parts::THUMB_JPEG)?;
    for (i, (name, xml)) in slide_parts.iter().enumerate() {
        w(&mut zip, &format!("ppt/slides/_rels/slide{}.xml.rels", i + 1), slide_rels_parts[i].1.as_bytes())?;
        w(&mut zip, name, xml.as_bytes())?;
    }
    for (part, bytes) in &media_parts {
        w(&mut zip, &format!("ppt/{}", part), bytes)?;
    }
    zip.finish()?;
    Ok(())
}

// ---------------------------------------------------------------- OPC 打包补丁

fn patch_content_types(slide_count: usize, media_map: &HashMap<String, String>) -> String {
    let mut ct = parts::CT_XML.to_string();
    let mut exts: Vec<String> = media_map
        .values()
        .filter_map(|p| p.rsplit('.').next().map(|e| e.to_lowercase()))
        .collect();
    exts.sort();
    exts.dedup();
    let ct_type = |ext: &str| match ext {
        "png" => "image/png",
        "jpeg" | "jpg" => "image/jpeg",
        "gif" => "image/gif",
        "bmp" => "image/bmp",
        "webp" => "image/webp",
        "tiff" | "tif" => "image/tiff",
        _ => "application/octet-stream",
    };
    let mut defaults = String::new();
    for e in &exts {
        defaults.push_str(&format!(r#"<Default Extension="{}" ContentType="{}"/>"#, e, ct_type(e)));
    }
    ct = ct.replace("<Override", &format!("{}<Override", defaults));
    let mut overrides = String::new();
    for i in 1..=slide_count {
        overrides.push_str(&format!(
            r#"<Override PartName="/ppt/slides/slide{}.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.slide+xml"/>"#,
            i
        ));
    }
    ct.replace("</Types>", &format!("{}</Types>", overrides))
}

fn patch_pres_rels(slide_count: usize) -> String {
    let rels = parts::PRES_RELS;
    let mut entries = String::new();
    for i in 0..slide_count {
        // 模板里 rId1-6 已占用，slides 从 rId7 起
        entries.push_str(&format!(
            r#"<Relationship Id="rId{}" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slide" Target="slides/slide{}.xml"/>"#,
            7 + i,
            i + 1
        ));
    }
    rels.replace("</Relationships>", &format!("{}</Relationships>", entries))
}

fn patch_presentation(manifest: &Presentation) -> String {
    let pres = parts::PRES_XML;
    let mut ids = String::new();
    for i in 0..manifest.slides.len() {
        ids.push_str(&format!(r#"<p:sldId id="{}" r:id="rId{}"/>"#, 256 + i, 7 + i));
    }
    let with_list = pres.replace(
        "</p:sldMasterIdLst>",
        &format!("</p:sldMasterIdLst><p:sldIdLst>{}</p:sldIdLst>", ids),
    );
    if let Some(start) = with_list.find("<p:sldSz") {
        if let Some(rel) = with_list[start..].find("/>") {
            let end = start + rel + 2;
            let mut s = with_list.clone();
            s.replace_range(
                start..end,
                &format!(r#"<p:sldSz cx="{}" cy="{}"/>"#, manifest.width_emu, manifest.height_emu),
            );
            return s;
        }
    }
    with_list
}

// ---------------------------------------------------------------- slide XML

fn emit_slide(slide: &Slide, media_map: &HashMap<String, String>, assets_dir: &Path) -> (String, String) {
    // 先给本页图片分配 rId（rId1 = layout，图片从 rId2 起）
    let mut rid_map: HashMap<String, u64> = HashMap::new();
    let mut next_rid = 2u64;
    for el in &slide.elements {
        if let Element::Image(img) = el {
            if !rid_map.contains_key(&img.asset) {
                rid_map.insert(img.asset.clone(), next_rid);
                next_rid += 1;
            }
        }
    }

    let mut inner = String::new();
    let mut id = 2u64; // spTree 的 cNvPr 占用 id=1
    for el in &slide.elements {
        let (xml, next_id) = emit_element(el, id, &rid_map, assets_dir);
        inner.push_str(&xml);
        id = next_id;
    }

    let bg_xml = slide
        .background_color
        .as_ref()
        .map(|bg| format!(r#"<p:bg><p:bgPr><a:solidFill><a:srgbClr val="{}"/></a:solidFill><a:effectLst/></p:bgPr></p:bg>"#, bg))
        .unwrap_or_default();

    let xml = format!(
        r#"<?xml version='1.0' encoding='UTF-8' standalone='yes'?><p:sld xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main"><p:cSld>{}<p:spTree><p:nvGrpSpPr><p:cNvPr id="1" name=""/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="0" cy="0"/><a:chOff x="0" y="0"/><a:chExt cx="0" cy="0"/></a:xfrm></p:grpSpPr>{}</p:spTree></p:cSld><p:clrMapOvr><a:masterClrMapping/></p:clrMapOvr></p:sld>"#,
        bg_xml, inner
    );

    let mut rels = String::new();
    rels.push_str(r#"<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideLayout" Target="../slideLayouts/slideLayout1.xml"/>"#);
    for el in &slide.elements {
        if let Element::Image(img) = el {
            let rid = rid_map[&img.asset];
            let part = media_map.get(&img.asset).cloned().unwrap_or_default();
            rels.push_str(&format!(
                r#"<Relationship Id="rId{}" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="../{}"/>"#,
                rid, part
            ));
        }
    }
    let rels_xml = format!(
        r#"<?xml version='1.0' encoding='UTF-8' standalone='yes'?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">{}</Relationships>"#,
        rels
    );
    (xml, rels_xml)
}

fn emit_element(el: &Element, mut id: u64, rid_map: &HashMap<String, u64>, assets_dir: &Path) -> (String, u64) {
    match el {
        Element::Text(t) => {
            let mut xml = format!(
                r#"<p:sp><p:nvSpPr><p:cNvPr id="{}" name="TextBox {}"/><p:cNvSpPr txBox="1"/><p:nvPr/></p:nvSpPr>"#,
                id, id
            );
            id += 1;
            xml.push_str(&format!(
                r#"<p:spPr>{}{}</p:spPr>"#,
                xfrm_xml(t.x, t.y, t.w, t.h, t.rotation, false, false),
                effect_xml(t.shadow.as_ref())
            ));
            xml.push_str(&tx_body_xml(
                &t.paragraphs,
                t.word_wrap,
                t.auto_size.as_deref(),
                t.anchor.as_deref(),
            ));
            xml.push_str("</p:sp>");
            (xml, id)
        }
        Element::Shape(sh) => {
            let mut xml = format!(
                r#"<p:sp><p:nvSpPr><p:cNvPr id="{}" name="Shape {}"/><p:cNvSpPr/><p:nvPr/></p:nvSpPr>"#,
                id, id
            );
            id += 1;
            let fill = match (sh.fill_type.as_deref(), &sh.fill_color) {
                (Some("solid") | None, Some(c)) => format!(r#"<a:solidFill><a:srgbClr val="{}"/></a:solidFill>"#, c),
                (Some("none"), _) => r#"<a:noFill/>"#.to_string(),
                _ => String::new(),
            };
            let mut line = String::new();
            if sh.line_color.is_some() || sh.line_width_pt.is_some() || sh.line_dash.is_some() {
                let w = sh
                    .line_width_pt
                    .map(|p| format!(r#" w="{}""#, (p * 12700.0).round() as i64))
                    .unwrap_or_default();
                let c = sh
                    .line_color
                    .as_ref()
                    .map(|c| format!(r#"<a:solidFill><a:srgbClr val="{}"/></a:solidFill>"#, c))
                    .unwrap_or_default();
                let d = sh
                    .line_dash
                    .as_ref()
                    .map(|d| format!(r#"<a:prstDash val="{}"/>"#, d))
                    .unwrap_or_default();
                line = format!(r#"<a:ln{}>{}{}</a:ln>"#, w, c, d);
            }
            let text = sh
                .paragraphs
                .as_ref()
                .map(|ps| tx_body_xml(ps, Some(true), None, Some("ctr")))
                .unwrap_or_default();
            xml.push_str(&format!(
                r#"<p:spPr>{}<a:prstGeom prst="{}"><a:avLst/></a:prstGeom>{}{}{}</p:spPr>"#,
                xfrm_xml(sh.x, sh.y, sh.w, sh.h, sh.rotation, false, false),
                sh.geometry,
                fill,
                line,
                effect_xml(sh.shadow.as_ref())
            ));
            if !text.is_empty() {
                xml.push_str(&text);
            }
            xml.push_str("</p:sp>");
            (xml, id)
        }
        Element::Image(img) => {
            let mut xml = format!(
                r#"<p:pic><p:nvPicPr><p:cNvPr id="{}" name="Image {}"/><p:cNvPicPr><a:picLocks noChangeAspect="1"/></p:cNvPicPr><p:nvPr/></p:nvPicPr>"#,
                id, id
            );
            id += 1;
            let rid = rid_map.get(&img.asset).copied().unwrap_or(2);
            let crop = img
                .crop_pct
                .as_ref()
                .map(|c| {
                    format!(
                        r#"<a:srcRect l="{}" t="{}" r="{}" b="{}"/>"#,
                        (c[0] * 1000.0) as i64,
                        (c[1] * 1000.0) as i64,
                        (c[2] * 1000.0) as i64,
                        (c[3] * 1000.0) as i64
                    )
                })
                .unwrap_or_default();
            let bytes = std::fs::read(assets_dir.join(&img.asset)).unwrap_or_default();
            let _ = bytes.len();
            xml.push_str(&format!(
                r#"<p:blipFill><a:blip r:embed="rId{}"/>{}<a:stretch><a:fillRect/></a:stretch></p:blipFill><p:spPr>{}<a:prstGeom prst="rect"><a:avLst/></a:prstGeom></p:spPr></p:pic>"#,
                rid,
                crop,
                xfrm_xml(img.x, img.y, img.w, img.h, img.rotation, false, false)
            ));
            (xml, id)
        }
        Element::Line(l) => {
            let mut xml = format!(
                r#"<p:cxnSp><p:nvCxnSpPr><p:cNvPr id="{}" name="Line {}"/><p:cNvCxnSpPr/><p:nvPr/></p:nvCxnSpPr>"#,
                id, id
            );
            id += 1;
            let mut ln = String::new();
            if l.color.is_some() || l.width_pt.is_some() || l.dash.is_some() || l.tail_end.is_some() {
                let w = l
                    .width_pt
                    .map(|p| format!(r#" w="{}""#, (p * 12700.0).round() as i64))
                    .unwrap_or_default();
                let c = l
                    .color
                    .as_ref()
                    .map(|c| format!(r#"<a:solidFill><a:srgbClr val="{}"/></a:solidFill>"#, c))
                    .unwrap_or_default();
                let d = l.dash.as_ref().map(|d| format!(r#"<a:prstDash val="{}"/>"#, d)).unwrap_or_default();
                let tail = l
                    .tail_end
                    .as_ref()
                    .filter(|t| t.as_str() != "none")
                    .map(|t| format!(r#"<a:tailEnd type="{}"/>"#, t))
                    .unwrap_or_default();
                ln = format!(r#"<a:ln{}>{}{}{}</a:ln>"#, w, c, d, tail);
            }
            xml.push_str(&format!(
                r#"<p:spPr>{}<a:prstGeom prst="line"><a:avLst/></a:prstGeom>{}</p:spPr></p:cxnSp>"#,
                xfrm_xml(l.x, l.y, l.w, l.h, None, l.flip_h, l.flip_v),
                ln
            ));
            (xml, id)
        }
        Element::Table(tb) => {
            let mut xml = format!(
                r#"<p:graphicFrame><p:nvGraphicFramePr><p:cNvPr id="{}" name="Table {}"/><p:cNvGraphicFramePr/><p:nvPr/></p:nvGraphicFramePr>"#,
                id, id
            );
            id += 1;
            xml.push_str(&format!(
                r#"<p:xfrm><a:off x="{}" y="{}"/><a:ext cx="{}" cy="{}"/></p:xfrm>"#,
                tb.x, tb.y, tb.w, tb.h
            ));
            let mut grid = String::new();
            for cw in &tb.col_widths {
                grid.push_str(&format!(r#"<a:gridCol w="{}"/>"#, cw));
            }
            let mut trs = String::new();
            for (ri, row) in tb.cells.iter().enumerate() {
                let h = tb.row_heights.get(ri).copied().unwrap_or(0);
                trs.push_str(&format!(r#"<a:tr h="{}">"#, h));
                for cell in row {
                    trs.push_str(&format!(
                        r#"<a:tc><a:txBody><a:bodyPr/><a:p><a:r><a:rPr lang="zh-CN"/><a:t>{}</a:t></a:r></a:p></a:txBody></a:tc>"#,
                        escape(cell)
                    ));
                }
                trs.push_str("</a:tr>");
            }
            xml.push_str(&format!(
                r#"<a:graphic><a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/table"><a:tbl><a:tblPr firstRow="1" bandRow="1"><a:tableStyleId>{{5C22544A-7EE6-4342-B048-85BDC9FD1C3A}}</a:tableStyleId></a:tblPr><a:tblGrid>{}</a:tblGrid>{}</a:tbl></a:graphicData></a:graphic></p:graphicFrame>"#,
                grid, trs
            ));
            (xml, id)
        }
        Element::Unsupported(u) => {
            // 占位矩形：视觉上留痕，第二轮转换后元素类型稳定
            let mut xml = format!(
                r#"<p:sp><p:nvSpPr><p:cNvPr id="{}" name="Unsupported"/><p:cNvSpPr/><p:nvPr/></p:nvSpPr>"#,
                id
            );
            id += 1;
            xml.push_str(&format!(
                r#"<p:spPr>{}<a:prstGeom prst="rect"><a:avLst/></a:prstGeom><a:noFill/><a:ln w="9525"><a:solidFill><a:srgbClr val="9AA7B8"/></a:solidFill><a:prstDash val="dash"/></a:ln></p:spPr>"#,
                xfrm_xml(u.x, u.y, u.w, u.h, None, false, false)
            ));
            xml.push_str(&format!(
                r#"<p:txBody><a:bodyPr anchor="ctr"/><a:p><a:r><a:rPr lang="zh-CN" sz="1000"><a:solidFill><a:srgbClr val="6B7A90"/></a:solidFill></a:rPr><a:t>[未支持: {}]</a:t></a:r></a:p></p:txBody></p:sp>"#,
                escape(&u.what)
            ));
            (xml, id)
        }
        _ => (String::new(), id),
    }
}

fn xfrm_xml(x: i64, y: i64, w: i64, h: i64, rotation: Option<f64>, flip_h: bool, flip_v: bool) -> String {
    let mut attrs = String::new();
    if let Some(r) = rotation {
        attrs.push_str(&format!(r#" rot="{}""#, (r * 60000.0).round() as i64));
    }
    if flip_h {
        attrs.push_str(r#" flipH="1""#);
    }
    if flip_v {
        attrs.push_str(r#" flipV="1""#);
    }
    format!(
        r#"<a:xfrm{}><a:off x="{}" y="{}"/><a:ext cx="{}" cy="{}"/></a:xfrm>"#,
        attrs, x, y, w, h
    )
}

fn effect_xml(shadow: Option<&Shadow>) -> String {
    match shadow {
        None => String::new(),
        Some(s) => format!(
            r#"<a:effectLst><a:outerShdw blurRad="{}" dist="{}" dir="{}" rotWithShape="0"><a:srgbClr val="{}"><a:alpha val="{}"/></a:srgbClr></a:outerShdw></a:effectLst>"#,
            (s.blur_pt * 12700.0).round() as i64,
            (s.dist_pt * 12700.0).round() as i64,
            (s.dir_deg * 60000.0).round() as i64,
            s.color,
            (s.alpha_pct.unwrap_or(100.0) * 1000.0).round() as i64
        ),
    }
}

fn tx_body_xml(paragraphs: &[Paragraph], word_wrap: Option<bool>, auto_size: Option<&str>, anchor: Option<&str>) -> String {
    let mut body_pr = String::from("<a:bodyPr");
    match word_wrap {
        Some(true) => body_pr.push_str(r#" wrap="square""#),
        Some(false) => body_pr.push_str(r#" wrap="none""#),
        None => {}
    }
    if let Some(a) = anchor {
        let v = match a {
            "top" => "t",
            "middle" => "ctr",
            "bottom" => "b",
            other => other,
        };
        body_pr.push_str(&format!(r#" anchor="{}""#, v));
    }
    body_pr.push('>');
    match auto_size {
        Some("none") => body_pr.push_str("<a:noAutofit/>"),
        Some("shape") => body_pr.push_str("<a:spAutoFit/>"),
        Some("text") => body_pr.push_str("<a:normAutofit/>"),
        _ => {}
    }
    body_pr.push_str("</a:bodyPr>");

    let mut ps = String::new();
    for para in paragraphs {
        let mut ppr = String::new();
        if let Some(a) = &para.align {
            let v = match a.as_str() {
                "left" => "l",
                "center" => "ctr",
                "right" => "r",
                "justify" => "just",
                other => other,
            };
            ppr.push_str(&format!(r#" algn="{}""#, v));
        }
        if let Some(l) = para.level {
            ppr.push_str(&format!(r#" lvl="{}""#, l));
        }
        let mut ppr_inner = String::new();
        if let Some(ls) = para.line_spacing {
            ppr_inner.push_str(&format!(
                r#"<a:lnSpc><a:spcPct val="{}"/></a:lnSpc>"#,
                (ls * 100000.0).round() as i64
            ));
        }
        let ppr_xml = if ppr.is_empty() && ppr_inner.is_empty() {
            String::new()
        } else {
            format!("<a:pPr{}>{}</a:pPr>", ppr, ppr_inner)
        };
        let mut runs_xml = String::new();
        for run in &para.runs {
            let mut rpr = String::from("<a:rPr lang=\"zh-CN\"");
            if let Some(sz) = run.size_pt {
                rpr.push_str(&format!(r#" sz="{}""#, (sz * 100.0).round() as i64));
            }
            if let Some(b) = run.bold {
                rpr.push_str(&format!(r#" b="{}""#, b as i64));
            }
            if let Some(i) = run.italic {
                rpr.push_str(&format!(r#" i="{}""#, i as i64));
            }
            if let Some(u) = run.underline {
                rpr.push_str(&format!(r#" u="{}""#, if u { "sng" } else { "none" }));
            }
            let mut rpr_inner = String::new();
            if let Some(c) = &run.color {
                rpr_inner.push_str(&format!(r#"<a:solidFill><a:srgbClr val="{}"/></a:solidFill>"#, c));
            }
            if let Some(f) = &run.font {
                rpr_inner.push_str(&format!(r#"<a:latin typeface="{}"/><a:ea typeface="{}"/>"#, f, f));
            }
            rpr.push_str(&format!(">{}</a:rPr>", rpr_inner));
            runs_xml.push_str(&format!("<a:r>{}<a:t>{}</a:t></a:r>", rpr, escape(&run.text)));
        }
        ps.push_str(&format!("<a:p>{}{}</a:p>", ppr_xml, runs_xml));
    }
    format!("<p:txBody>{}{}</p:txBody>", body_pr, ps)
}
