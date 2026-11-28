//! manifest → PPTX 写入器。用嵌入的空白模板打底，注入 slides / media / rels / content-types。
//! v0.2：链接、渐变、自由形状、合并表格、备注页、主题回填、unsupported 标记化。
use super::parts;
use crate::model::*;
use crate::xmlutil::escape;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub fn write_pptx(manifest: &Presentation, assets_dir: &Path, out: &Path) -> Result<(), crate::error::BridgeError> {
    let file = std::fs::File::create(out)?;
    let mut zip = zip::ZipWriter::new(file);
    let options = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);

    // 1. 媒体映射：asset 文件名 → 包内路径（元素图片 + 每页背景图）
    // 母版包已占用 image1..N → 幻灯片媒体从最大编号之后起数，避免同路径不同内容
    let pack_media_max = manifest
        .template
        .as_ref()
        .map(|p| {
            p.media
                .iter()
                .filter_map(|m| file_name_part(&m.path).strip_prefix("image").and_then(|s| s.parse::<i64>().ok()))
                .max()
                .unwrap_or(0)
        })
        .unwrap_or(0);
    let mut media_map: HashMap<String, String> = HashMap::new();
    let mut media_parts: Vec<(String, Vec<u8>)> = Vec::new();
    let mut next_media: i64 = pack_media_max + 1;
    let add_asset = |asset: &String,
                         media_map: &mut HashMap<String, String>,
                         media_parts: &mut Vec<(String, Vec<u8>)>,
                         next_media: &mut i64| {
        if media_map.contains_key(asset) {
            return;
        }
        let ext = PathBuf::from(asset)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("png")
            .to_lowercase();
        let part = format!("media/image{}.{}", next_media, ext);
        *next_media += 1;
        media_map.insert(asset.clone(), part.clone());
        let bytes = std::fs::read(assets_dir.join(asset)).unwrap_or_default();
        media_parts.push((part, bytes));
    };
    for slide in &manifest.slides {
        for el in &slide.elements {
            if let Element::Image(img) = el {
                add_asset(&img.asset, &mut media_map, &mut media_parts, &mut next_media);
            }
        }
        if let Some(bg) = &slide.background_image {
            add_asset(bg, &mut media_map, &mut media_parts, &mut next_media);
        }
    }

    // 2. 逐页 XML + rels
    let mut slide_parts: Vec<String> = Vec::new();
    let mut slide_rels_parts: Vec<String> = Vec::new();
    for (i, slide) in manifest.slides.iter().enumerate() {
        let (xml, rels) = emit_slide(slide, i + 1, &media_map);
        slide_parts.push(xml);
        slide_rels_parts.push(rels);
    }

    // 3. 主题回填 / 母版资源包 + 备注
    let pack = manifest.template.as_ref().filter(|p| !p.master_xml.is_empty());
    let theme_xml = match pack {
        Some(p) => p.theme_xml.clone(), // 母版包自带 raw 主题，整块使用
        None => patch_theme(manifest.theme.as_ref()),
    };
    let notes_numbers: Vec<usize> = manifest
        .slides
        .iter()
        .enumerate()
        .filter(|(_, s)| s.notes.is_some())
        .map(|(i, _)| i + 1)
        .collect();
    let notes_present = !notes_numbers.is_empty();

    // 打包前把母版媒体并入写入清单（按包内路径去重，幻灯片媒体优先）
    let mut all_media: Vec<(String, Vec<u8>)> = std::mem::take(&mut media_parts);
    if let Some(p) = pack {
        for m in &p.media {
            let bytes = std::fs::read(assets_dir.join(&m.asset)).unwrap_or_default();
            if !all_media.iter().any(|(path, _)| path == &m.path) {
                all_media.push((m.path.clone(), bytes));
            }
        }
    }

    // 4. 打包
    let layout_numbers: Vec<u32> = pack
        .map(|p| p.layouts.iter().map(|l| l.number).collect())
        .unwrap_or_else(|| (1..=parts::LAYOUT_XML.len() as u32).collect());
    let ct = patch_content_types(manifest, &media_map, &layout_numbers);
    let pres_rels = patch_pres_rels(manifest.slides.len(), notes_present);
    let pres = patch_presentation(manifest, notes_present, pack.and_then(|p| p.default_text_style.as_deref()));

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
    match pack {
        Some(p) => {
            // 母版资源包：master / layouts / 母版侧媒体 全部按原编号与路径回写
            w(&mut zip, "ppt/slideMasters/slideMaster1.xml", p.master_xml.as_bytes())?;
            w(&mut zip, "ppt/slideMasters/_rels/slideMaster1.xml.rels", p.master_rels_xml.as_bytes())?;
            for lp in &p.layouts {
                w(&mut zip, &format!("ppt/slideLayouts/slideLayout{}.xml", lp.number), lp.xml.as_bytes())?;
                if !lp.rels_xml.is_empty() {
                    w(&mut zip, &format!("ppt/slideLayouts/_rels/slideLayout{}.xml.rels", lp.number), lp.rels_xml.as_bytes())?;
                }
            }
        }
        None => {
            w(&mut zip, "ppt/slideMasters/slideMaster1.xml", parts::MASTER_XML.as_bytes())?;
            w(&mut zip, "ppt/slideMasters/_rels/slideMaster1.xml.rels", parts::MASTER_RELS.as_bytes())?;
            for (i, xml) in parts::LAYOUT_XML.iter().enumerate() {
                w(&mut zip, &format!("ppt/slideLayouts/slideLayout{}.xml", i + 1), xml.as_bytes())?;
                w(&mut zip, &format!("ppt/slideLayouts/_rels/slideLayout{}.xml.rels", i + 1), parts::LAYOUT_RELS[i].as_bytes())?;
            }
        }
    }
    w(&mut zip, "ppt/theme/theme1.xml", theme_xml.as_bytes())?;
    w(&mut zip, "ppt/presProps.xml", parts::PRES_PROPS.as_bytes())?;
    w(&mut zip, "ppt/viewProps.xml", parts::VIEW_PROPS.as_bytes())?;
    w(&mut zip, "ppt/tableStyles.xml", parts::TABLE_STYLES.as_bytes())?;
    w(&mut zip, "ppt/printerSettings/printerSettings1.bin", parts::PRINTER_BIN)?;
    w(&mut zip, "docProps/thumbnail.jpeg", parts::THUMB_JPEG)?;
    for (i, xml) in slide_parts.iter().enumerate() {
        w(&mut zip, &format!("ppt/slides/_rels/slide{}.xml.rels", i + 1), slide_rels_parts[i].as_bytes())?;
        w(&mut zip, &format!("ppt/slides/slide{}.xml", i + 1), xml.as_bytes())?;
    }
    for (part, bytes) in &all_media {
        if part.starts_with("ppt/") {
            w(&mut zip, part, bytes)?;
        } else {
            w(&mut zip, &format!("ppt/{}", part), bytes)?;
        }
    }
    if notes_present {
        w(&mut zip, "ppt/theme/theme2.xml", theme_xml.as_bytes())?;
        w(&mut zip, "ppt/notesMasters/notesMaster1.xml", parts::NOTES_MASTER_XML.as_bytes())?;
        w(&mut zip, "ppt/notesMasters/_rels/notesMaster1.xml.rels", NOTES_MASTER_RELS.as_bytes())?;
        for n in &notes_numbers {
            let text = manifest.slides[*n - 1].notes.clone().unwrap_or_default();
            w(&mut zip, &format!("ppt/notesSlides/notesSlide{}.xml", n), notes_slide_xml(&text).as_bytes())?;
            w(&mut zip, &format!("ppt/notesSlides/_rels/notesSlide{}.xml.rels", n), notes_slide_rels(*n).as_bytes())?;
        }
    }
    zip.finish()?;
    Ok(())
}

// ---------------------------------------------------------------- OPC 打包补丁

fn patch_content_types(manifest: &Presentation, media_map: &HashMap<String, String>, layout_numbers: &[u32]) -> String {
    let mut ct = parts::CT_XML.to_string();
    let mut exts: Vec<String> = media_map
        .values()
        .filter_map(|p| p.rsplit('.').next().map(|e| e.to_lowercase()))
        .collect();
    // 母版包媒体也有扩展名
    if let Some(p) = &manifest.template {
        for m in &p.media {
            if let Some(e) = m.path.rsplit('.').next().map(|e| e.to_lowercase()) {
                exts.push(e);
            }
        }
    }
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
    for i in 1..=manifest.slides.len() {
        overrides.push_str(&format!(
            r#"<Override PartName="/ppt/slides/slide{}.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.slide+xml"/>"#,
            i
        ));
    }
    // 版式覆盖项：按实际数量重写（模板里硬编码了 11 个）
    for i in 1..=11 {
        ct = ct.replace(
            &format!(
                r#"<Override PartName="/ppt/slideLayouts/slideLayout{i}.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.slideLayout+xml"/>"#
            ),
            "",
        );
    }
    for n in layout_numbers {
        overrides.push_str(&format!(
            r#"<Override PartName="/ppt/slideLayouts/slideLayout{n}.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.slideLayout+xml"/>"#
        ));
    }
    let notes: Vec<usize> = manifest
        .slides
        .iter()
        .enumerate()
        .filter(|(_, s)| s.notes.is_some())
        .map(|(i, _)| i + 1)
        .collect();
    if !notes.is_empty() {
        overrides.push_str(&format!(
            r#"<Override PartName="/ppt/theme/theme2.xml" ContentType="application/vnd.openxmlformats-officedocument.theme+xml"/><Override PartName="/ppt/notesMasters/notesMaster1.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.notesMaster+xml"/>"#
        ));
        for n in &notes {
            overrides.push_str(&format!(
                r#"<Override PartName="/ppt/notesSlides/notesSlide{}.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.notesSlide+xml"/>"#,
                n
            ));
        }
    }
    ct.replace("</Types>", &format!("{}</Types>", overrides))
}

fn patch_pres_rels(slide_count: usize, notes_present: bool) -> String {
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
    if notes_present {
        entries.push_str(&format!(
            r#"<Relationship Id="rId{}" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/notesMaster" Target="notesMasters/notesMaster1.xml"/>"#,
            7 + slide_count
        ));
    }
    rels.replace("</Relationships>", &format!("{}</Relationships>", entries))
}

fn patch_presentation(manifest: &Presentation, notes_present: bool, default_text_style: Option<&str>) -> String {
    let pres = parts::PRES_XML;
    let mut ids = String::new();
    for i in 0..manifest.slides.len() {
        ids.push_str(&format!(r#"<p:sldId id="{}" r:id="rId{}"/>"#, 256 + i, 7 + i));
    }
    let notes_lst = if notes_present {
        format!(
            r#"<p:notesMasterIdLst><p:notesMasterId id="2147483649" r:id="rId{}"/></p:notesMasterIdLst>"#,
            7 + manifest.slides.len()
        )
    } else {
        String::new()
    };
    let with_list = pres.replace(
        "</p:sldMasterIdLst>",
        &format!("</p:sldMasterIdLst>{}<p:sldIdLst>{}</p:sldIdLst>", notes_lst, ids),
    );
    if let Some(start) = with_list.find("<p:sldSz") {
        if let Some(rel) = with_list[start..].find("/>") {
            let end = start + rel + 2;
            let mut s = with_list.clone();
            s.replace_range(
                start..end,
                &format!(r#"<p:sldSz cx="{}" cy="{}"/>"#, manifest.width_emu, manifest.height_emu),
            );
            // 源文件的默认文本样式（影响无显式样式段落的行距/字号继承）
            if let Some(dts) = default_text_style {
                if let (Some(a), Some(b)) = (s.find("<p:defaultTextStyle"), s.find("</p:defaultTextStyle>")) {
                    let close_end = b + "</p:defaultTextStyle>".len();
                    s.replace_range(a..close_end, dts);
                }
            }
            return s;
        }
    }
    with_list
}

/// manifest 里的主题 → 回填 theme1.xml（clrScheme 颜色 / 字体 / 名称）。
/// name=None 时移除 name 属性（读侧 None ⇄ 写侧无属性，往返一致）。
fn patch_theme(theme: Option<&ThemeManifest>) -> String {
    let mut s = parts::THEME_XML.to_string();
    let Some(t) = theme else { return s };

    // ---- clrScheme 块
    let scheme_start = match s.find("<a:clrScheme") {
        Some(i) => i,
        None => return s,
    };
    const SCHEME_END: &str = "</a:clrScheme>";
    let scheme_end = match s[scheme_start..].find(SCHEME_END) {
        Some(i) => scheme_start + i + SCHEME_END.len(),
        None => return s,
    };
    let mut block = s[scheme_start..scheme_end].to_string();
    // 开标签（含 name 属性）
    if let Some(tag_end) = block.find('>') {
        let new_open = match &t.name {
            Some(n) => format!(r#"<a:clrScheme name="{}">"#, escape(n)),
            None => "<a:clrScheme>".to_string(),
        };
        block.replace_range(..=tag_end, &new_open);
    }
    const SLOTS: [&str; 12] = [
        "dk1", "lt1", "dk2", "lt2", "accent1", "accent2", "accent3", "accent4", "accent5",
        "accent6", "hlink", "folHlink",
    ];
    let vals = [
        &t.colors.dk1,
        &t.colors.lt1,
        &t.colors.dk2,
        &t.colors.lt2,
        &t.colors.accent1,
        &t.colors.accent2,
        &t.colors.accent3,
        &t.colors.accent4,
        &t.colors.accent5,
        &t.colors.accent6,
        &t.colors.hlink,
        &t.colors.fol_hlink,
    ];
    for (slot, val) in SLOTS.iter().zip(vals.iter()) {
        if let Some(v) = val.as_ref() {
            let open = format!("<a:{slot}>");
            let close = format!("</a:{slot}>");
            if let (Some(a), Some(b)) = (block.find(&open), block.find(&close)) {
                let replacement = format!(r#"<a:{slot}><a:srgbClr val="{v}"/></a:{slot}>"#);
                block.replace_range(a..(b + close.len()), &replacement);
            }
        }
    }
    s.replace_range(scheme_start..scheme_end, &block);

    // ---- 字体（latin typeface 的值）
    for (tag, val) in [("<a:majorFont>", &t.major_font), ("<a:minorFont>", &t.minor_font)] {
        let Some(v) = val else { continue };
        let close_tag = format!("</a:{}>", &tag[3..tag.len() - 1]);
        if let (Some(a), Some(b)) = (s.find(tag), s.find(&close_tag)) {
            let mut blk = s[a..b].to_string();
            const LATIN: &str = "<a:latin typeface=\"";
            if let Some(la) = blk.find(LATIN) {
                let vs = la + LATIN.len();
                if let Some(le) = blk[vs..].find('"') {
                    blk.replace_range(vs..(vs + le), v);
                }
            }
            s.replace_range(a..b, &blk);
        }
    }
    s
}

// ---------------------------------------------------------------- 备注 parts

const NOTES_MASTER_RELS: &str = r#"<?xml version='1.0' encoding='UTF-8' standalone='yes'?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/theme" Target="../theme/theme2.xml"/></Relationships>"#;

fn notes_slide_rels(n: usize) -> String {
    format!(
        r#"<?xml version='1.0' encoding='UTF-8' standalone='yes'?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slide" Target="../slides/slide{n}.xml"/><Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/notesMaster" Target="../notesMasters/notesMaster1.xml"/></Relationships>"#
    )
}

fn notes_slide_xml(text: &str) -> String {
    let mut paras = String::new();
    for line in text.split('\n') {
        if line.is_empty() {
            paras.push_str(r#"<a:p><a:endParaRPr lang="zh-CN"/></a:p>"#);
        } else {
            paras.push_str(&format!(
                r#"<a:p><a:r><a:rPr lang="zh-CN" dirty="0"/><a:t>{}</a:t></a:r></a:p>"#,
                escape(line)
            ));
        }
    }
    format!(
        r#"<?xml version='1.0' encoding='UTF-8' standalone='yes'?><p:notes xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main"><p:cSld><p:spTree><p:nvGrpSpPr><p:cNvPr id="1" name=""/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="0" cy="0"/><a:chOff x="0" y="0"/><a:chExt cx="0" cy="0"/></a:xfrm></p:grpSpPr><p:sp><p:nvSpPr><p:cNvPr id="2" name="Slide Image Placeholder 1"/><p:cNvSpPr><a:spLocks noGrp="1" noRot="1" noChangeAspect="1"/></p:cNvSpPr><p:nvPr><p:ph type="sldImg"/></p:nvPr></p:nvSpPr><p:spPr/></p:sp><p:sp><p:nvSpPr><p:cNvPr id="3" name="Notes Placeholder 1"/><p:cNvSpPr><a:spLocks noGrp="1"/></p:cNvSpPr><p:nvPr><p:ph type="body" idx="1"/></p:nvPr></p:nvSpPr><p:spPr/><p:txBody><a:bodyPr/><a:lstStyle/>{paras}</p:txBody></p:sp></p:spTree></p:cSld><p:clrMapOvr><a:masterClrMapping/></p:clrMapOvr></p:notes>"#
    )
}

// ---------------------------------------------------------------- slide XML

/// 链接的 rel 目标：(Target, 是否外部)。空 Link（既无 url 也无 slide）→ None，不建 rel。
fn link_target_of(link: &Link) -> Option<(String, bool)> {
    if let Some(u) = &link.url {
        Some((escape(u), true))
    } else if let Some(n) = link.slide {
        Some((format!("slide{}.xml", n), false))
    } else {
        None
    }
}

/// 段落里的链接引用（与元素遍历同序，供 rid 分配与发射共用）
fn push_para_links<'a>(ps: &'a [Paragraph], out: &mut Vec<&'a Link>) {
    for p in ps {
        for r in &p.runs {
            if let Some(l) = &r.link {
                out.push(l);
            }
        }
    }
}

/// 发射期间的链接 rid 供给：rids 只含合法链接，cursor 与遍历同步推进
struct LinkCtx<'a> {
    rids: &'a [u64],
    cursor: usize,
}

impl LinkCtx<'_> {
    fn next_rid(&mut self, link: &Link) -> Option<u64> {
        if link_target_of(link).is_some() {
            let rid = self.rids.get(self.cursor).copied().unwrap_or(0);
            self.cursor += 1;
            Some(rid)
        } else {
            None
        }
    }
}

fn emit_slide(slide: &Slide, idx1: usize, media_map: &HashMap<String, String>) -> (String, String) {
    // 1. 链接清单（与元素遍历同序）
    let mut link_list: Vec<&Link> = Vec::new();
    for el in &slide.elements {
        match el {
            Element::Text(t) => push_para_links(&t.paragraphs, &mut link_list),
            Element::Shape(s) => {
                if let Some(ps) = &s.paragraphs {
                    push_para_links(ps, &mut link_list);
                }
            }
            Element::Image(img) => {
                if let Some(l) = &img.link {
                    link_list.push(l);
                }
            }
            _ => {}
        }
    }

    // 2. rid 分配：rId1 = layout；背景图 → 元素图片 → 链接
    let mut next_rid = 2u64;
    let bg_rid: Option<u64> = slide.background_image.as_ref().map(|_| {
        let r = next_rid;
        next_rid += 1;
        r
    });
    let mut rid_map: HashMap<String, u64> = HashMap::new();
    for el in &slide.elements {
        if let Element::Image(img) = el {
            if !rid_map.contains_key(&img.asset) {
                rid_map.insert(img.asset.clone(), next_rid);
                next_rid += 1;
            }
        }
    }
    let mut link_rids: Vec<u64> = Vec::new();
    for l in &link_list {
        if link_target_of(l).is_some() {
            link_rids.push(next_rid);
            next_rid += 1;
        }
    }

    // 3. 元素
    let mut inner = String::new();
    let mut id = 2u64; // spTree 的 cNvPr 占用 id=1
    let mut links = LinkCtx { rids: &link_rids, cursor: 0 };
    for el in &slide.elements {
        let (xml, next_id) = emit_element(el, id, &rid_map, &mut links);
        inner.push_str(&xml);
        id = next_id;
    }

    // 4. 背景
    let bg_xml = if let (_, Some(rid)) = (&slide.background_image, bg_rid) {
        format!(
            r#"<p:bg><p:bgPr><a:blipFill><a:blip r:embed="rId{rid}"/><a:stretch><a:fillRect/></a:stretch></a:blipFill><a:effectLst/></p:bgPr></p:bg>"#
        )
    } else if let Some(bg) = &slide.background_color {
        format!(r#"<p:bg><p:bgPr><a:solidFill><a:srgbClr val="{}"/></a:solidFill><a:effectLst/></p:bgPr></p:bg>"#, bg)
    } else {
        String::new()
    };

    let xml = format!(
        r#"<?xml version='1.0' encoding='UTF-8' standalone='yes'?><p:sld xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main"><p:cSld>{}<p:spTree><p:nvGrpSpPr><p:cNvPr id="1" name=""/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="0" cy="0"/><a:chOff x="0" y="0"/><a:chExt cx="0" cy="0"/></a:xfrm></p:grpSpPr>{}</p:spTree></p:cSld><p:clrMapOvr><a:masterClrMapping/></p:clrMapOvr></p:sld>"#,
        bg_xml, inner
    );

    // 5. rels
    let layout_no = slide.layout.unwrap_or(1);
    let mut rels = String::new();
    rels.push_str(&format!(
        r#"<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideLayout" Target="../slideLayouts/slideLayout{layout_no}.xml"/>"#
    ));
    if let (Some(bg_asset), Some(rid)) = (&slide.background_image, bg_rid) {
        let part = media_map.get(bg_asset).cloned().unwrap_or_default();
        rels.push_str(&format!(
            r#"<Relationship Id="rId{}" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="../{}"/>"#,
            rid, part
        ));
    }
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
    let mut li = 0usize;
    for l in &link_list {
        if let Some((target, external)) = link_target_of(l) {
            let rid = link_rids[li];
            li += 1;
            let mode = if external { r#" TargetMode="External""# } else { "" };
            rels.push_str(&format!(
                r#"<Relationship Id="rId{}" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/hyperlink" Target="{}"{}/>"#,
                rid, target, mode
            ));
        }
    }
    // 备注页关系（读侧据此提取备注正文）
    if slide.notes.is_some() {
        rels.push_str(&format!(
            r#"<Relationship Id="rId{}" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/notesSlide" Target="../notesSlides/notesSlide{}.xml"/>"#,
            next_rid, idx1
        ));
    }
    let rels_xml = format!(
        r#"<?xml version='1.0' encoding='UTF-8' standalone='yes'?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">{}</Relationships>"#,
        rels
    );
    (xml, rels_xml)
}

fn emit_element(el: &Element, mut id: u64, rid_map: &HashMap<String, u64>, links: &mut LinkCtx) -> (String, u64) {
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
                t.insets,
                t.font_scale_pct,
                t.ln_spc_reduction_pct,
                links,
            ));
            xml.push_str("</p:sp>");
            (xml, id)
        }
        Element::Shape(sh) => {
            let ph = match (&sh.ph_type, sh.ph_idx) {
                (Some(t), Some(i)) => format!(r#"<p:nvPr><p:ph type="{t}" idx="{i}"/></p:nvPr>"#),
                (Some(t), None) => format!(r#"<p:nvPr><p:ph type="{t}"/></p:nvPr>"#),
                (None, Some(i)) => format!(r#"<p:nvPr><p:ph idx="{i}"/></p:nvPr>"#),
                _ => "<p:nvPr/>".to_string(),
            };
            let mut xml = format!(
                r#"<p:sp><p:nvSpPr><p:cNvPr id="{}" name="Shape {}"/><p:cNvSpPr/>{}</p:nvSpPr>"#,
                id, id, ph
            );
            id += 1;
            let fill = fill_xml(
                sh.fill_type.as_deref(),
                sh.fill_color.as_ref(),
                sh.fill_alpha_pct,
                sh.fill_stops.as_deref(),
                sh.fill_angle_deg,
            );
            let line = line_xml(sh.line_color.as_ref(), sh.line_width_pt, sh.line_dash.as_ref(), None, None);
            let text = sh
                .paragraphs
                .as_ref()
                .map(|ps| {
                    // 形状缺省 ctr（None 语义 = ctr，读侧归一）
                    let anchor = sh.anchor.as_deref().or(Some("ctr"));
                    tx_body_xml(ps, Some(true), None, anchor, sh.insets, None, None, links)
                })
                .unwrap_or_default();
            let avlst = match sh.adj_val {
                Some(v) => format!(r#"<a:avLst><a:gd name="adj" fmla="val {}"/></a:avLst>"#, v),
                None => "<a:avLst/>".to_string(),
            };
            if sh.inherit_geom {
                // 几何继承版式：spPr 留空
                xml.push_str("<p:spPr/>");
            } else {
                xml.push_str(&format!(
                    r#"<p:spPr>{}<a:prstGeom prst="{}">{}</a:prstGeom>{}{}{}</p:spPr>"#,
                    xfrm_xml(sh.x, sh.y, sh.w, sh.h, sh.rotation, false, false),
                    sh.geometry,
                    avlst,
                    fill,
                    line,
                    effect_xml(sh.shadow.as_ref())
                ));
            }
            if !text.is_empty() {
                xml.push_str(&text);
            }
            xml.push_str("</p:sp>");
            (xml, id)
        }
        Element::Image(img) => {
            let mut xml = format!(
                r#"<p:pic><p:nvPicPr><p:cNvPr id="{}" name="Image {}">{}{}</p:cNvPr><p:cNvPicPr><a:picLocks noChangeAspect="1"/></p:cNvPicPr><p:nvPr/></p:nvPicPr>"#,
                id,
                id,
                hlink_for(img.link.as_ref(), links),
                img.link
                    .as_ref()
                    .and_then(|l| l.tooltip.as_ref())
                    .map(|t| format!(r#" descr="{}""#, escape(t)))
                    .unwrap_or_default()
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
            let ln = line_xml(
                l.color.as_ref(),
                l.width_pt,
                l.dash.as_ref(),
                l.head_end.as_ref(),
                l.tail_end.as_ref(),
            );
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
            // 垂直覆盖表：col → 该列还被上方 rowSpan 延续盖住几行
            let max_cols = tb.cells.iter().map(|r| r.len()).max().unwrap_or(0).max(tb.col_widths.len());
            let mut v_cover: Vec<i64> = vec![0; max_cols.max(1)];
            for (ri, row) in tb.cells.iter().enumerate() {
                let h = tb.row_heights.get(ri).copied().unwrap_or(0);
                trs.push_str(&format!(r#"<a:tr h="{}">"#, h));
                // 本行左侧 origin 的 gridSpan 还要吞掉几格（这些格不推进列游标）
                let mut remaining_cover = 0usize;
                let mut ci = 0usize;
                for cell in row {
                    let c = ci.min(v_cover.len().saturating_sub(1));
                    let span = cell.col_span.map(|s| s.max(1) as usize).unwrap_or(1);
                    if cell.merged_away && remaining_cover > 0 {
                        // 被左侧 gridSpan 隐含的横向延续格：跳过，列游标不动
                        remaining_cover -= 1;
                        continue;
                    }
                    let attrs;
                    if !cell.merged_away {
                        let mut a = String::new();
                        if let Some(cs) = cell.col_span {
                            a.push_str(&format!(r#" gridSpan="{}""#, cs));
                        }
                        if let Some(rs) = cell.row_span {
                            a.push_str(&format!(r#" rowSpan="{}""#, rs));
                        }
                        let rs = cell.row_span.unwrap_or(1).max(1) as i64;
                        if rs > 1 {
                            for j in 0..span.min(v_cover.len().saturating_sub(c)) {
                                v_cover[c + j] = rs - 1;
                            }
                        }
                        attrs = a;
                    } else if v_cover[c] > 0 {
                        // 纵向延续格：显式发射，PowerPoint 才能把上方单元格画下来
                        let mut a = String::from(r#" vMerge="1""#);
                        if let Some(cs) = cell.col_span {
                            a.push_str(&format!(r#" gridSpan="{}""#, cs));
                        }
                        for j in 0..span.min(v_cover.len().saturating_sub(c)) {
                            v_cover[c + j] -= 1;
                        }
                        attrs = a;
                    } else {
                        // 显式横向延续（hMerge 编码）
                        let mut a = String::from(r#" hMerge="1""#);
                        if let Some(cs) = cell.col_span {
                            a.push_str(&format!(r#" gridSpan="{}""#, cs));
                        }
                        attrs = a;
                    }
                    emit_tc(&mut trs, cell, &attrs);
                    if span > 1 {
                        remaining_cover = span - 1;
                    }
                    ci += span;
                }
                trs.push_str("</a:tr>");
            }
            xml.push_str(&format!(
                r#"<a:graphic><a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/table"><a:tbl><a:tblPr firstRow="1" bandRow="1"><a:tableStyleId>{{5C22544A-7EE6-4342-B048-85BDC9FD1C3A}}</a:tableStyleId></a:tblPr><a:tblGrid>{}</a:tblGrid>{}</a:tbl></a:graphicData></a:graphic></p:graphicFrame>"#,
                grid, trs
            ));
            (xml, id)
        }
        Element::Freeform(f) => {
            let mut xml = format!(
                r#"<p:sp><p:nvSpPr><p:cNvPr id="{}" name="Freeform {}"/><p:cNvSpPr/><p:nvPr/></p:nvSpPr>"#,
                id, id
            );
            id += 1;
            let pw = f.path_w.unwrap_or_else(|| f.w.max(1));
            let ph = f.path_h.unwrap_or_else(|| f.h.max(1));
            let mut paths = String::new();
            for sp in &f.subpaths {
                let wa = sp.w.map(|v| format!(r#" w="{}""#, v)).unwrap_or_default();
                let ha = sp.h.map(|v| format!(r#" h="{}""#, v)).unwrap_or_default();
                paths.push_str(&format!("<a:path{}{}>", wa, ha));
                for cmd in &sp.cmds {
                    match cmd {
                        PathCmd::Move { x, y } => paths.push_str(&format!(r#"<a:moveTo><a:pt x="{x}" y="{y}"/></a:moveTo>"#)),
                        PathCmd::Line { x, y } => paths.push_str(&format!(r#"<a:lnTo><a:pt x="{x}" y="{y}"/></a:lnTo>"#)),
                        PathCmd::Cubic { x1, y1, x2, y2, x, y } => paths.push_str(&format!(
                            r#"<a:cubicBezTo><a:pt x="{x1}" y="{y1}"/><a:pt x="{x2}" y="{y2}"/><a:pt x="{x}" y="{y}"/></a:cubicBezTo>"#
                        )),
                        PathCmd::Quad { x1, y1, x, y } => paths.push_str(&format!(
                            r#"<a:quadBezTo><a:pt x="{x1}" y="{y1}"/><a:pt x="{x}" y="{y}"/></a:quadBezTo>"#
                        )),
                        PathCmd::Arc { w_r, h_r, st_ang_deg, sw_ang_deg } => paths.push_str(&format!(
                            r#"<a:arcTo wR="{w_r}" hR="{h_r}" stAng="{}" swAng="{}"/>"#,
                            (st_ang_deg * 60000.0).round() as i64,
                            (sw_ang_deg * 60000.0).round() as i64
                        )),
                    }
                }
                if sp.closed {
                    paths.push_str("<a:close/>");
                }
                paths.push_str("</a:path>");
            }
            let fill = fill_xml(
                f.fill_type.as_deref(),
                f.fill_color.as_ref(),
                None,
                None,
                None,
            );
            let line = line_xml(f.line_color.as_ref(), f.line_width_pt, None, None, None);
            xml.push_str(&format!(
                r#"<p:spPr>{}<a:custGeom><a:avLst/><a:rect l="0" t="0" r="{pw}" b="{ph}"/><a:pathLst>{paths}</a:pathLst></a:custGeom>{}{}</p:spPr></p:sp>"#,
                xfrm_xml(f.x, f.y, f.w, f.h, f.rotation, false, false),
                fill,
                line,
            ));
            (xml, id)
        }
        Element::Unsupported(u) => {
            // 隐形标记形状：读侧按 name 前缀还原 → 任意 deck 往返 STABLE
            let mut xml = format!(
                r#"<p:sp><p:nvSpPr><p:cNvPr id="{id}" name="phb:unsupported:{}" descr="{}"/><p:cNvSpPr/><p:nvPr/></p:nvSpPr>"#,
                escape(&u.what),
                escape(&u.note),
            );
            id += 1;
            xml.push_str(&format!(
                r#"<p:spPr>{}<a:prstGeom prst="rect"><a:avLst/></a:prstGeom><a:noFill/><a:ln><a:noFill/></a:ln></p:spPr></p:sp>"#,
                xfrm_xml(u.x, u.y, u.w, u.h, None, false, false)
            ));
            (xml, id)
        }
    }
}

/// cNvPr 上的图片超链接子元素
fn hlink_for(link: Option<&Link>, links: &mut LinkCtx) -> String {
    match link {
        Some(l) => match links.next_rid(l) {
            Some(rid) => {
                let tip = l
                    .tooltip
                    .as_ref()
                    .map(|t| format!(r#" tooltip="{}""#, escape(t)))
                    .unwrap_or_default();
                format!(r#"<a:hlinkClick r:id="rId{}"{}/>"#, rid, tip)
            }
            None => String::new(),
        },
        None => String::new(),
    }
}

/// run 级超链接（消费一个链接 rid）
fn hlink_run(link: &Link, links: &mut LinkCtx) -> String {
    match links.next_rid(link) {
        Some(rid) => {
            let tip = link
                .tooltip
                .as_ref()
                .map(|t| format!(r#" tooltip="{}""#, escape(t)))
                .unwrap_or_default();
            format!(r#"<a:hlinkClick r:id="rId{}"{}/>"#, rid, tip)
        }
        None => String::new(),
    }
}

fn fill_xml(
    fill_type: Option<&str>,
    fill_color: Option<&String>,
    fill_alpha_pct: Option<f64>,
    stops: Option<&[GradientStop]>,
    angle_deg: Option<f64>,
) -> String {
    match fill_type {
        Some("none") => r#"<a:noFill/>"#.to_string(),
        Some("gradient") => {
            let mut gs = String::new();
            if let Some(list) = stops {
                for s in list {
                    let alpha = s
                        .alpha_pct
                        .map(|a| format!(r#"<a:alpha val="{}"/>"#, (a * 1000.0).round() as i64))
                        .unwrap_or_default();
                    gs.push_str(&format!(
                        r#"<a:gs pos="{}"><a:srgbClr val="{}">{}</a:srgbClr></a:gs>"#,
                        (s.pos_pct * 1000.0).round() as i64,
                        s.color,
                        alpha
                    ));
                }
            }
            if gs.is_empty() {
                return r#"<a:noFill/>"#.to_string();
            }
            let lin = match angle_deg {
                Some(a) => format!(r#"<a:lin ang="{}" scaled="1"/>"#, (a * 60000.0).round() as i64),
                None => String::new(),
            };
            format!(r#"<a:gradFill rotWithShape="1"><a:gsLst>{gs}</a:gsLst>{lin}</a:gradFill>"#)
        }
        // solid（含 None = 按有颜色即实心）
        _ => match fill_color {
            Some(c) => {
                let alpha = fill_alpha_pct
                    .map(|a| format!(r#"<a:alpha val="{}"/>"#, (a * 1000.0).round() as i64))
                    .unwrap_or_default();
                format!(r#"<a:solidFill><a:srgbClr val="{}">{}</a:srgbClr></a:solidFill>"#, c, alpha)
            }
            None => String::new(),
        },
    }
}

fn line_xml(
    color: Option<&String>,
    width_pt: Option<f64>,
    dash: Option<&String>,
    head_end: Option<&String>,
    tail_end: Option<&String>,
) -> String {
    if color.is_some() || width_pt.is_some() || dash.is_some() || head_end.is_some() || tail_end.is_some() {
        let w = width_pt
            .map(|p| format!(r#" w="{}""#, (p * 12700.0).round() as i64))
            .unwrap_or_default();
        let c = color
            .as_ref()
            .map(|c| format!(r#"<a:solidFill><a:srgbClr val="{}"/></a:solidFill>"#, c))
            .unwrap_or_default();
        let d = dash
            .as_ref()
            .map(|d| format!(r#"<a:prstDash val="{}"/>"#, d))
            .unwrap_or_default();
        let he = head_end
            .as_ref()
            .filter(|t| t.as_str() != "none")
            .map(|t| format!(r#"<a:headEnd type="{}"/>"#, t))
            .unwrap_or_default();
        let te = tail_end
            .as_ref()
            .filter(|t| t.as_str() != "none")
            .map(|t| format!(r#"<a:tailEnd type="{}"/>"#, t))
            .unwrap_or_default();
        format!(r#"<a:ln{}>{}{}{}{}</a:ln>"#, w, c, d, he, te)
    } else {
        String::new()
    }
}

/// 发射一个 `<a:tc>`（含 txBody 与可选底色）
fn emit_tc(trs: &mut String, cell: &TableCell, attrs: &str) {
    let tcpr = cell
        .fill_color
        .as_ref()
        .map(|c| format!(r#"<a:tcPr><a:solidFill><a:srgbClr val="{c}"/></a:solidFill></a:tcPr>"#))
        .unwrap_or_default();
    trs.push_str(&format!(
        r#"<a:tc{}><a:txBody><a:bodyPr/><a:lstStyle/>{}</a:txBody>{}</a:tc>"#,
        attrs,
        table_cell_body(cell),
        tcpr
    ));
}

/// 表格单元格的 txBody 内容（\n 分段；样式作用于每段，读侧取首段 → 往返一致）
fn table_cell_body(cell: &TableCell) -> String {
    let mut txb = String::new();
    for line in cell.text.split('\n') {
        let mut ppr = String::new();
        if let Some(a) = &cell.align {
            let v = match a.as_str() {
                "left" => "l",
                "center" => "ctr",
                "right" => "r",
                other => other,
            };
            ppr.push_str(&format!(r#" algn="{}""#, v));
        }
        let ppr_xml = if ppr.is_empty() { String::new() } else { format!("<a:pPr{}/>", ppr) };
        if line.is_empty() {
            txb.push_str(&format!(r#"<a:p>{}<a:endParaRPr lang="zh-CN"/></a:p>"#, ppr_xml));
        } else {
            let mut rpr = String::from(r#"<a:rPr lang="zh-CN""#);
            if let Some(b) = cell.bold {
                rpr.push_str(&format!(r#" b="{}""#, b as i64));
            }
            if let Some(sz) = cell.size_pt {
                rpr.push_str(&format!(r#" sz="{}""#, (sz * 100.0).round() as i64));
            }
            rpr.push('>');
            if let Some(c) = &cell.color {
                rpr.push_str(&format!(r#"<a:solidFill><a:srgbClr val="{}"/></a:solidFill>"#, c));
            }
            rpr.push_str("</a:rPr>");
            txb.push_str(&format!(
                r#"<a:p>{}<a:r>{}<a:t>{}</a:t></a:r></a:p>"#,
                ppr_xml,
                rpr,
                escape(line)
            ));
        }
    }
    txb
}

/// 路径的文件名（不含扩展名）
fn file_name_part(p: &str) -> &str {
    let f = p.rsplit('/').next().unwrap_or(p);
    match f.rfind('.') {
        Some(i) => &f[..i],
        None => f,
    }
}

fn xfrm_xml(x: i64, y: i64, w: i64, h: i64, rotation: Option<f64>, flip_h: bool, flip_v: bool) -> String {    let mut attrs = String::new();
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
        Some(s) => {
            let algn = s
                .algn
                .as_ref()
                .map(|a| format!(r#" algn="{}""#, a))
                .unwrap_or_default();
            format!(
                r#"<a:effectLst><a:outerShdw blurRad="{}" dist="{}" dir="{}"{} rotWithShape="0"><a:srgbClr val="{}"><a:alpha val="{}"/></a:srgbClr></a:outerShdw></a:effectLst>"#,
                (s.blur_pt * 12700.0).round() as i64,
                (s.dist_pt * 12700.0).round() as i64,
                (s.dir_deg * 60000.0).round() as i64,
                algn,
                s.color,
                (s.alpha_pct.unwrap_or(100.0) * 1000.0).round() as i64
            )
        }
    }
}

fn tx_body_xml(
    paragraphs: &[Paragraph],
    word_wrap: Option<bool>,
    auto_size: Option<&str>,
    anchor: Option<&str>,
    insets: Option<[i64; 4]>,
    font_scale_pct: Option<f64>,
    ln_spc_reduction_pct: Option<f64>,
    links: &mut LinkCtx,
) -> String {
    let mut body_pr = String::from("<a:bodyPr");
    match word_wrap {
        Some(true) => body_pr.push_str(r#" wrap="square""#),
        Some(false) => body_pr.push_str(r#" wrap="none""#),
        None => {}
    }
    if let Some([l, t, r, b]) = insets {
        body_pr.push_str(&format!(r#" lIns="{l}" tIns="{t}" rIns="{r}" bIns="{b}""#));
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
        Some("text") => {
            let fs = font_scale_pct.map(|v| format!(r#" fontScale="{}""#, (v * 1000.0).round() as i64)).unwrap_or_default();
            let lr = ln_spc_reduction_pct
                .map(|v| format!(r#" lnSpcReduction="{}""#, (v * 1000.0).round() as i64))
                .unwrap_or_default();
            body_pr.push_str(&format!("<a:normAutofit{}{}/>", fs, lr));
        }
        _ => {}
    }
    body_pr.push_str("</a:bodyPr>");

    let mut ps = String::new();
    for para in paragraphs {
        let mut ppr = String::new();
        if let Some(ml) = para.mar_l {
            ppr.push_str(&format!(r#" marL="{}""#, ml));
        }
        if let Some(ind) = para.indent {
            ppr.push_str(&format!(r#" indent="{}""#, ind));
        }
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
        if let Some(sb) = para.space_before_pt {
            ppr_inner.push_str(&format!(
                r#"<a:spcBef><a:spcPts val="{}"/></a:spcBef>"#,
                (sb * 100.0).round() as i64
            ));
        }
        if let Some(sa) = para.space_after_pt {
            ppr_inner.push_str(&format!(
                r#"<a:spcAft><a:spcPts val="{}"/></a:spcAft>"#,
                (sa * 100.0).round() as i64
            ));
        }
        // 项目符号（schema 顺序：buClr → buSzPct → buFont → bu*）
        if let Some(b) = &para.bullet {
            if let Some(c) = &b.color {
                ppr_inner.push_str(&format!(r#"<a:buClr><a:srgbClr val="{}"/></a:buClr>"#, c));
            }
            if let Some(sz) = b.size_pct {
                ppr_inner.push_str(&format!(r#"<a:buSzPct val="{}"/>"#, (sz * 1000.0).round() as i64));
            }
            if let Some(f) = &b.font {
                ppr_inner.push_str(&format!(r#"<a:buFont typeface="{}"/>"#, escape(f)));
            }
            match b.kind.as_str() {
                "none" => ppr_inner.push_str("<a:buNone/>"),
                "auto_num" => {
                    let ty = b
                        .num_type
                        .as_ref()
                        .map(|t| format!(r#" type="{}""#, t))
                        .unwrap_or_default();
                    let st = b
                        .num_start
                        .map(|s| format!(r#" startAt="{}""#, s))
                        .unwrap_or_default();
                    ppr_inner.push_str(&format!("<a:buAutoNum{}{}/>", ty, st));
                }
                // char（缺省）
                _ => {
                    let ch = b
                        .character
                        .as_ref()
                        .map(|c| format!(r#" char="{}""#, escape(c)))
                        .unwrap_or_else(|| r#" char="•""#.to_string());
                    ppr_inner.push_str(&format!("<a:buChar{}/>", ch));
                }
            }
        }
        let ppr_xml = if ppr.is_empty() && ppr_inner.is_empty() {
            String::new()
        } else {
            format!("<a:pPr{}>{}</a:pPr>", ppr, ppr_inner)
        };
        let mut runs_xml = String::new();
        for run in &para.runs {
            // a:br：换行以专门的 br 元素表达（读侧还原为 "\n" 文本段）
            if run.text == "\n" {
                let rpr = run_rpr(run, links);
                runs_xml.push_str(&format!("<a:br>{}</a:br>", rpr));
                continue;
            }
            let rpr = run_rpr(run, links);
            runs_xml.push_str(&format!("<a:r>{}<a:t>{}</a:t></a:r>", rpr, escape(&run.text)));
        }
        ps.push_str(&format!("<a:p>{}{}</a:p>", ppr_xml, runs_xml));
    }
    format!("<p:txBody>{}{}</p:txBody>", body_pr, ps)
}

fn run_rpr(run: &TextRun, links: &mut LinkCtx) -> String {
    let lang = run.lang.as_deref().unwrap_or("zh-CN");
    let mut rpr = format!("<a:rPr lang=\"{}\"", escape(lang));
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
    if let Some(k) = run.kern {
        rpr.push_str(&format!(r#" kern="{}""#, k));
    }
    if let Some(s) = run.spc {
        rpr.push_str(&format!(r#" spc="{}""#, s));
    }
    let mut rpr_inner = String::new();
    if let Some(c) = &run.color {
        rpr_inner.push_str(&format!(r#"<a:solidFill><a:srgbClr val="{}"/></a:solidFill>"#, c));
    }
    if let Some(f) = &run.font {
        let pitch = run
            .font_pitch_family
            .map(|v| format!(r#" pitchFamily="{}""#, v))
            .unwrap_or_default();
        let panose = run
            .font_panose
            .as_ref()
            .map(|v| format!(r#" panose="{}""#, escape(v)))
            .unwrap_or_default();
        let cs_lat = run.font_charset.map(|v| format!(r#" charset="{}""#, v)).unwrap_or_default();
        let cs_ea = run
            .font_charset_ea
            .map(|v| format!(r#" charset="{}""#, v))
            .unwrap_or_default();
        rpr_inner.push_str(&format!(
            r#"<a:latin typeface="{}"{}/><a:ea typeface="{}"{}/>"#,
            escape(f),
            format!("{pitch}{panose}{cs_lat}"),
            escape(f),
            format!("{pitch}{panose}{cs_ea}")
        ));
        if let Some(cs) = &run.font_cs {
            rpr_inner.push_str(&format!(r#"<a:cs typeface="{}"/>"#, escape(cs)));
        }
        if let Some(sym) = &run.font_sym {
            rpr_inner.push_str(&format!(r#"<a:sym typeface="{}"/>"#, escape(sym)));
        }
    }
    if let Some(l) = &run.link {
        rpr_inner.push_str(&hlink_run(l, links));
    }
    rpr.push_str(&format!(">{}</a:rPr>", rpr_inner));
    rpr
}
