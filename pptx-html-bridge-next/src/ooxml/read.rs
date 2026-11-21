//! PPTX → manifest 读取器。zip + roxmltree，不走 python-pptx。
use super::theme::Theme;
use crate::xmlutil::*;
use crate::error::{BridgeError, Result};
use crate::model::*;
use roxmltree::Node;
use std::collections::HashMap;
use std::path::Path;

pub struct ReadOutcome {
    pub manifest: Presentation,
    pub fidelity: Vec<String>,
    pub assets: Vec<(String, Vec<u8>)>, // (asset 文件名, 字节)
}

struct Ctx<'a> {
    zip: &'a mut zip::ZipArchive<std::fs::File>,
    theme: Theme,
    fidelity: Vec<String>,
    assets: Vec<(String, Vec<u8>)>,
    media_done: HashMap<String, String>, // 包内路径 → asset 文件名
}

impl Ctx<'_> {
    fn part_str(&mut self, part: &str) -> Result<String> {
        let mut f = self.zip.by_name(part)?;
        let mut s = String::new();
        use std::io::Read;
        f.read_to_string(&mut s)?;
        Ok(s)
    }
    fn note(&mut self, msg: impl Into<String>) {
        let m = msg.into();
        if !self.fidelity.iter().any(|x| x == &m) {
            self.fidelity.push(m);
        }
    }
    /// 把媒体 part 提取为 asset，按内容哈希命名（跨轮次稳定 + 自动去重）
    fn extract_media(&mut self, part: String) -> String {
        if let Some(name) = self.media_done.get(&part) {
            return name.clone();
        }
        let ext = file_name(&part)
            .rsplit('.')
            .next()
            .unwrap_or("png")
            .to_lowercase();
        let bytes = self
            .zip
            .by_name(&part)
            .map(|mut f| {
                let mut b = Vec::new();
                use std::io::Read;
                f.read_to_end(&mut b).ok();
                b
            })
            .unwrap_or_default();
        let name = format!("img_{}.{}", fnv1a(&bytes), ext);
        self.assets.push((name.clone(), bytes.clone()));
        self.media_done.insert(part.clone(), name.clone());
        name
    }
}

// ---------------------------------------------------------------- 路径

fn parent_dir(p: &str) -> &str {
    match p.rfind('/') {
        Some(i) => &p[..i],
        None => "",
    }
}

fn file_name(p: &str) -> &str {
    match p.rfind('/') {
        Some(i) => &p[i + 1..],
        None => p,
    }
}

fn resolve(base_dir: &str, target: &str) -> String {
    if let Some(stripped) = target.strip_prefix('/') {
        return stripped.to_string();
    }
    let mut parts: Vec<&str> = if base_dir.is_empty() { vec![] } else { base_dir.split('/').collect() };
    for seg in target.split('/') {
        match seg {
            "." => {}
            ".." => { parts.pop(); }
            s => parts.push(s),
        }
    }
    parts.join("/")
}

// ---------------------------------------------------------------- 入口

pub fn read_pptx(path: &Path) -> Result<ReadOutcome> {
    let file = std::fs::File::open(path)?;
    let mut zip = zip::ZipArchive::new(file)?;
    let mut ctx = Ctx {
        zip: &mut zip,
        theme: Theme::default(),
        fidelity: vec![],
        assets: vec![],
        media_done: HashMap::new(),
    };

    // 1. 根 rels → officeDocument part
    let root_rels = ctx.part_str("_rels/.rels")?;
    let doc = roxmltree::Document::parse(&root_rels)?;
    let pres_target: String = doc
        .root_element()
        .children()
        .find(|n: &roxmltree::Node| {
            attr(*n, "Type").map(|t| t.ends_with("/officeDocument")).unwrap_or(false)
        })
        .and_then(|n| attr(n, "Target"))
        .map(|t| resolve("", t).clone())
        .unwrap_or_default();
    if pres_target.is_empty() {
        return Err(BridgeError::Other("根 rels 里找不到 officeDocument".into()));
    }
    let pres_part = pres_target;
    let pres_base = parent_dir(&pres_part);

    // 2. presentation.xml + 其 rels
    let pres_xml = ctx.part_str(&pres_part)?;
    let pres_doc = roxmltree::Document::parse(&pres_xml)?;
    let root = pres_doc.root_element();
    let sld_sz = find(root, "sldSz");
    let (width_emu, height_emu) = match &sld_sz {
        Some(n) => (
            attr(*n, "cx").and_then(|v| v.parse().ok()).unwrap_or(12192000),
            attr(*n, "cy").and_then(|v| v.parse().ok()).unwrap_or(6858000),
        ),
        None => (12192000, 6858000),
    };
    let sld_ids: Vec<String> = find(root, "sldIdLst")
        .map(|lst| {
            lst.children()
                .filter(|n| n.tag_name().name() == "sldId")
                .filter_map(|n| attr_ns(n, NS_R, "id"))
                .map(|s| s.to_string())
                .collect()
        })
        .unwrap_or_default();

    let pres_rels_path = format!("{}/_rels/{}.rels", pres_base, file_name(&pres_part));
    let pres_rels = ctx.part_str(&pres_rels_path)?;
    let pres_rels_doc = roxmltree::Document::parse(&pres_rels)?;
    let mut rel_map: HashMap<String, String> = HashMap::new(); // rId → 完整 part 路径
    let mut rel_type: HashMap<String, String> = HashMap::new();
    for r in pres_rels_doc.root_element().children().filter(|n| n.tag_name().name() == "Relationship") {
        if let (Some(id), Some(target), Some(ty)) = (attr(r, "Id"), attr(r, "Target"), attr(r, "Type")) {
            rel_map.insert(id.to_string(), resolve(pres_base, target));
            rel_type.insert(id.to_string(), ty.to_string());
        }
    }

    // 3. theme + master clrMap
    let theme_part = rel_type
        .iter()
        .find(|(_, t)| t.ends_with("/theme"))
        .map(|(rid, _)| rel_map[rid].clone());
    let master_part = rel_type
        .iter()
        .find(|(_, t)| t.ends_with("/slideMaster"))
        .map(|(rid, _)| rel_map[rid].clone());
    let theme_xml = match &theme_part {
        Some(p) => ctx.part_str(p)?,
        None => String::new(),
    };
    let master_xml = match &master_part {
        Some(p) => Some(ctx.part_str(p)?),
        None => None,
    };
    ctx.theme = Theme::parse(&theme_xml, master_xml.as_deref());

    // 4. 逐页解析
    let mut slides = Vec::new();
    for (i, rid) in sld_ids.iter().enumerate() {
        let slide_part = rel_map
            .get(rid)
            .ok_or_else(|| BridgeError::Other(format!("slide rId {rid} 无对应 part")))?;
        let slide_base = parent_dir(slide_part);
        let slide_rels_path = format!("{}/_rels/{}.rels", slide_base, file_name(slide_part));

        let mut slide_rels: HashMap<String, String> = HashMap::new();
        if ctx.zip.by_name(&slide_rels_path).is_ok() {
            let rels_xml = ctx.part_str(&slide_rels_path)?;
            let rd = roxmltree::Document::parse(&rels_xml)?;
            for r in rd.root_element().children().filter(|n| n.tag_name().name() == "Relationship") {
                if let (Some(id), Some(target)) = (attr(r, "Id"), attr(r, "Target")) {
                    slide_rels.insert(id.to_string(), resolve(slide_base, target));
                }
            }
        } else {
            ctx.note("有 slide 缺少 rels part");
        }

        let xml = ctx.part_str(slide_part)?;
        let sdoc = roxmltree::Document::parse(&xml)?;
        let root_el = sdoc.root_element();

        // 背景（bg 与 spTree 都在 cSld 里）
        let background_color = find(root_el, "bg")
            .and_then(|bg| child(bg, "bgPr"))
            .and_then(|bgpr| child(bgpr, "solidFill"))
            .and_then(|sf| child(sf, "srgbClr"))
            .and_then(|c| attr(c, "val"))
            .map(|v| v.to_uppercase());

        // spTree
        let t = T::identity();
        let mut elements = Vec::new();
        if let Some(sp_tree) = find(root_el, "spTree") {
            for c in sp_tree.children().filter(|n| n.is_element()) {
                read_any(c, &mut ctx, i, &slide_rels, &t, &mut elements);
            }
        }

        // notes 提示
        if slide_rels
            .values()
            .any(|t| t.ends_with(".xml") && t.contains("notesSlide"))
        {
            ctx.note(format!("第 {} 页的备注页已跳过（v0.1 未支持）", i + 1));
        }

        slides.push(Slide { elements, background_color, notes: None });
    }

    if ctx.fidelity.is_empty() {
        ctx.fidelity.push("转换全程无损".into());
    }

    Ok(ReadOutcome {
        manifest: Presentation { kind: "presentation".into(), width_emu, height_emu, slides },
        fidelity: ctx.fidelity,
        assets: ctx.assets,
    })
}

// ---------------------------------------------------------------- spTree 遍历

#[derive(Clone, Copy, Debug)]
struct T {
    ox: i64,
    oy: i64,
    chx: i64,
    chy: i64,
    sx: f64,
    sy: f64,
}

impl T {
    fn identity() -> T {
        T { ox: 0, oy: 0, chx: 0, chy: 0, sx: 1.0, sy: 1.0 }
    }
    fn map_x(&self, lx: i64) -> i64 {
        (self.ox as f64 + (lx - self.chx) as f64 * self.sx).round() as i64
    }
    fn map_y(&self, ly: i64) -> i64 {
        (self.oy as f64 + (ly - self.chy) as f64 * self.sy).round() as i64
    }
    fn dim(&self, d: i64, horizontal: bool) -> i64 {
        let s = if horizontal { self.sx } else { self.sy };
        (d as f64 * s).round() as i64
    }
}

#[derive(Debug, Clone, Copy)]
struct Geom {
    x: i64,
    y: i64,
    w: i64,
    h: i64,
    rot: Option<f64>,
    flip_h: bool,
    flip_v: bool,
    chx: i64,
    chy: i64,
}

fn read_xfrm(node: Node) -> Option<Geom> {
    read_geom_of(node)
}

fn read_geom_of(node: Node) -> Option<Geom> {
    // xfrm 在 spPr 里（sp/pic/cxnSp）；graphicFrame 的 p:xfrm 由 read_gframe 单独处理
    let sp_pr = child(node, "spPr")?;
    let xf = child(sp_pr, "xfrm")?;
    let off = child(xf, "off")?;
    let ext = child(xf, "ext")?;
    let x = attr(off, "x").and_then(|v| v.parse().ok()).unwrap_or(0);
    let y = attr(off, "y").and_then(|v| v.parse().ok()).unwrap_or(0);
    let w = attr(ext, "cx").and_then(|v| v.parse().ok()).unwrap_or(0);
    let h = attr(ext, "cy").and_then(|v| v.parse().ok()).unwrap_or(0);
    let rot = attr(xf, "rot").map(|v| v.parse::<f64>().unwrap_or(0.0) / 60000.0);
    let flip_h = attr(xf, "flipH").map(|v| v == "1").unwrap_or(false);
    let flip_v = attr(xf, "flipV").map(|v| v == "1").unwrap_or(false);
    Some(Geom { x, y, w, h, rot, flip_h, flip_v, chx: 0, chy: 0 })
}

// ---------------------------------------------------------------- 元素分发

fn read_any(node: Node, ctx: &mut Ctx, slide_idx: usize, slide_rels: &HashMap<String, String>, t: &T, out: &mut Vec<Element>) {
    let local = node.tag_name().name();
    match local {
        "sp" => read_sp(node, ctx, slide_idx, slide_rels, t, out),
        "pic" => read_pic(node, ctx, slide_rels, t, out),
        "cxnSp" => read_cxn(node, t, out),
        "graphicFrame" => read_gframe(node, t, out),
        "grpSp" => read_grp(node, ctx, slide_idx, slide_rels, t, out),
        "AlternateContent" => {
            // PowerPoint 的兼容包装：找第一个真实形状后代
            let inner = node
                .descendants()
                .find(|d| matches!(d.tag_name().name(), "sp" | "pic" | "cxnSp" | "graphicFrame" | "grpSp"));
            if let Some(inner) = inner {
                read_any(inner, ctx, slide_idx, slide_rels, t, out);
            }
        }
        _ => {}
    }
}


fn read_group_xfrm(node: Node) -> Option<(Geom, i64, i64)> {
    let g = read_geom_of(node)?;
    let xf = child(node, "xfrm")?;
    let chx = child(xf, "chOff")
        .and_then(|o| attr(o, "x"))
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    let chy = child(xf, "chOff")
        .and_then(|o| attr(o, "y"))
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    Some((g, chx, chy))
}

fn read_sp(node: Node, ctx: &mut Ctx, slide_idx: usize, slide_rels: &HashMap<String, String>, t: &T, out: &mut Vec<Element>) {
    let geom = match read_geom_of(node) {
        Some(g) => g,
        None => return,
    };
    let (x, y, w, h) = (t.map_x(geom.x), t.map_y(geom.y), t.dim(geom.w, true), t.dim(geom.h, false));

    let sp_pr = child(node, "spPr");
    let prst = sp_pr
        .and_then(|sp| child(sp, "prstGeom"))
        .and_then(|g| attr(g, "prst"))
        .unwrap_or("rect")
        .to_string();
    let cust_geom = sp_pr.and_then(|sp| child(sp, "custGeom")).is_some();

    let tx_box = node
        .descendants()
        .find(|d| d.tag_name().name() == "cNvSpPr")
        .and_then(|d| attr(d, "txBox"))
        .map(|v| v == "1")
        .unwrap_or(false);

    // 颜色读取
    let fill_color = sp_pr.and_then(|sp| child(sp, "solidFill")).and_then(|sf| read_color_child(sf, ctx));
    let (line_color, line_width_pt, line_dash, tail_end) = sp_pr
        .and_then(|sp| child(sp, "ln"))
        .map(|ln| {
            let c = child(ln, "solidFill").and_then(|sf| read_color_child(sf, ctx));
            let w = attr(ln, "w").and_then(|v| v.parse::<f64>().ok()).map(|e| e / 12700.0);
            let d = child(ln, "prstDash").and_then(|d| attr(d, "val")).map(|v| v.to_string());
            let te = child(ln, "tailEnd").and_then(|d| attr(d, "type")).map(|v| v.to_string());
            (c, w, d, te)
        })
        .unwrap_or((None, None, None, None));
    let shadow = sp_pr.and_then(|sp| child(sp, "effectLst")).and_then(|e| child(e, "outerShdw")).map(read_shadow);

    let paragraphs = child(node, "txBody").map(|tb| read_paragraphs(tb, ctx));

    if cust_geom {
        ctx.note(format!(
            "第 {} 页：自定义几何形状降级为矩形（位置/文字保留）",
            slide_idx + 1
        ));
    }

    if prst == "line" {
        out.push(Element::Line(LineElement {
            x, y, w, h,
            color: line_color,
            width_pt: line_width_pt,
            dash: line_dash,
            flip_h: geom.flip_h,
            flip_v: geom.flip_v,
            tail_end,
        }));
        return;
    }

    if tx_box {
        out.push(Element::Text(TextElement {
            x, y, w, h,
            paragraphs: paragraphs.unwrap_or_default(),
            rotation: geom.rot,
            word_wrap: read_word_wrap(node),
            auto_size: read_auto_size(node),
            anchor: read_anchor(node),
            shadow,
        }));
        return;
    }

    // 普通形状
    if fill_color.is_none() && sp_pr.and_then(|sp| child(sp, "solidFill")).is_some() {
        ctx.note(format!("第 {} 页：主题/渐变填充已折算或降级", slide_idx + 1));
    }
    out.push(Element::Shape(ShapeElement {
        x, y, w, h,
        geometry: prst,
        fill_type: Some(if fill_color.is_some() { "solid".to_string() } else { "none".to_string() }),
        fill_color,
        line_color,
        line_width_pt,
        line_dash,
        rotation: geom.rot,
        shadow,
        paragraphs,
    }));
}

fn read_color_child(fill_node: Node, ctx: &mut Ctx) -> Option<String> {
    let c = fill_node.children().find(|n| n.is_element())?;
    match c.tag_name().name() {
        "srgbClr" => attr(c, "val").map(|v| v.to_uppercase()),
        "schemeClr" => {
            ctx.note("theme/scheme 颜色已折算为 RGB");
            ctx.theme.resolve_scheme_node(c)
        }
        "sysClr" => attr(c, "lastClr").map(|v| v.to_uppercase()),
        "prstClr" => {
            ctx.note("预设颜色（prstClr）已跳过");
            None
        }
        _ => None,
    }
}

fn read_shadow(n: Node) -> Shadow {
    Shadow {
        blur_pt: attr(n, "blurRad").and_then(|v| v.parse::<f64>().ok()).map(|e| e / 12700.0).unwrap_or(0.0),
        dist_pt: attr(n, "dist").and_then(|v| v.parse::<f64>().ok()).map(|e| e / 12700.0).unwrap_or(0.0),
        dir_deg: attr(n, "dir").and_then(|v| v.parse::<f64>().ok()).map(|e| e / 60000.0).unwrap_or(0.0),
        color: child(n, "srgbClr")
            .and_then(|c| attr(c, "val"))
            .map(|v| v.to_uppercase())
            .unwrap_or_else(|| "000000".into()),
        alpha_pct: child(n, "srgbClr")
            .and_then(|c| child(c, "alpha"))
            .and_then(|a| attr(a, "val"))
            .and_then(|v| v.parse().ok())
            .map(|v: f64| v / 1000.0),
    }
}

fn read_word_wrap(sp_node: Node) -> Option<bool> {
    let bp = find(sp_node, "bodyPr")?;
    attr(bp, "wrap").map(|v| v != "none")
}

fn read_auto_size(sp_node: Node) -> Option<String> {
    let bp = find(sp_node, "bodyPr")?;
    if child(bp, "spAutoFit").is_some() {
        Some("shape".into())
    } else if child(bp, "normAutofit").is_some() {
        Some("text".into())
    } else if child(bp, "noAutofit").is_some() {
        Some("none".into())
    } else {
        None
    }
}

fn read_anchor(sp_node: Node) -> Option<String> {
    let bp = find(sp_node, "bodyPr")?;
    attr(bp, "anchor").map(|v| match v {
        "t" => "top".to_string(),
        "b" => "bottom".to_string(),
        _ => "middle".to_string(),
    })
}

fn read_paragraphs(tx_body: Node, ctx: &mut Ctx) -> Vec<Paragraph> {
    let mut out = Vec::new();
    for p in children(tx_body, "p") {
        let mut runs = Vec::new();
        for r in children(p, "r") {
            let text = child(r, "t").and_then(|t| t.text()).unwrap_or("").to_string();
            let rpr = child(r, "rPr");
            let size_pt = rpr
                .and_then(|rp| attr(rp, "sz"))
                .and_then(|v| v.parse::<f64>().ok())
                .map(|v| v / 100.0);
            let color = rpr
                .and_then(|rp| child(rp, "solidFill"))
                .and_then(|sf| read_color_child(sf, ctx));
            let font = rpr
                .and_then(|rp| child(rp, "latin"))
                .and_then(|l| attr(l, "typeface"))
                .map(|v| v.to_string())
                .or_else(|| {
                    rpr.and_then(|rp| child(rp, "ea")).and_then(|l| attr(l, "typeface")).map(|v| v.to_string())
                });
            let bold = rpr.and_then(|rp| attr(rp, "b")).map(|v| v == "1");
            let italic = rpr.and_then(|rp| attr(rp, "i")).map(|v| v == "1");
            let underline = rpr.and_then(|rp| attr(rp, "u")).map(|v| v != "none");
            runs.push(TextRun { text, bold, italic, underline, size_pt, color, font });
        }
        let ppr = child(p, "pPr");
        let align = ppr
            .and_then(|pp| attr(pp, "algn"))
            .map(|v| match v {
                "l" => "left".to_string(),
                "ctr" => "center".to_string(),
                "r" => "right".to_string(),
                "just" => "justify".to_string(),
                other => other.to_string(),
            });
        let level = ppr.and_then(|pp| attr(pp, "lvl")).and_then(|v| v.parse().ok());
        let line_spacing = ppr
            .and_then(|pp| child(pp, "lnSpc"))
            .and_then(|ls| child(ls, "spcPct"))
            .and_then(|sp| attr(sp, "val"))
            .and_then(|v| v.parse::<f64>().ok())
            .map(|v| v / 100000.0);
        out.push(Paragraph { runs, align, level, line_spacing });
    }
    out
}

fn read_pic(node: Node, ctx: &mut Ctx, slide_rels: &HashMap<String, String>, t: &T, out: &mut Vec<Element>) {
    let geom = match read_geom_of(node) {
        Some(g) => g,
        None => return,
    };
    let (x, y, w, h) = (t.map_x(geom.x), t.map_y(geom.y), t.dim(geom.w, true), t.dim(geom.h, false));
    let rid = node
        .descendants()
        .find(|d| d.tag_name().name() == "blip")
        .and_then(|b| attr_ns(b, NS_R, "embed"))
        .map(|s| s.to_string());
    let asset = rid
        .and_then(|rid| slide_rels.get(&rid).cloned())
        .map(|part| ctx.extract_media(part))
        .unwrap_or_default();

    let crop_pct = node
        .descendants()
        .find(|d| d.tag_name().name() == "srcRect")
        .map(|sr| {
            let f = |a: &str| attr(sr, a).and_then(|v| v.parse::<f64>().ok()).map(|v| v / 1000.0);
            [
                f("l").unwrap_or(0.0),
                f("t").unwrap_or(0.0),
                f("r").unwrap_or(0.0),
                f("b").unwrap_or(0.0),
            ]
        });

    out.push(Element::Image(ImageElement {
        x, y, w, h,
        asset,
        rotation: geom.rot,
        crop_pct: crop_pct.filter(|c| c.iter().any(|v| *v != 0.0)),
    }));
}

fn read_cxn(node: Node, t: &T, out: &mut Vec<Element>) {
    let geom = match read_geom_of(node) {
        Some(g) => g,
        None => return,
    };
    let (x, y, w, h) = (t.map_x(geom.x), t.map_y(geom.y), t.dim(geom.w, true), t.dim(geom.h, false));
    let (color, width_pt, dash) = child(node, "spPr")
        .and_then(|sp| child(sp, "ln"))
        .map(|ln| {
            let c = child(ln, "solidFill").and_then(|sf| child(sf, "srgbClr")).and_then(|c| attr(c, "val")).map(|v| v.to_uppercase());
            let w = attr(ln, "w").and_then(|v| v.parse::<f64>().ok()).map(|e| e / 12700.0);
            let d = child(ln, "prstDash").and_then(|d| attr(d, "val")).map(|v| v.to_string());
            (c, w, d)
        })
        .unwrap_or((None, None, None));
    let ln_node = child(node, "spPr").and_then(|sp| child(sp, "ln"));
    if ln_node.is_none() {
        eprintln!("DEBUG cxn: no ln");
    }
    if let Some(ln) = ln_node {
        let te = child(ln, "tailEnd");
        eprintln!("DEBUG cxn: ln children = {:?}, tailEnd = {:?}", ln.children().map(|c| c.tag_name().name().to_string()).collect::<Vec<_>>(), te.map(|t| attr(t, "type")));
    }
    let tail_end = child(node, "spPr")
        .and_then(|sp| child(sp, "ln"))
        .and_then(|ln| child(ln, "tailEnd"))
        .and_then(|te| attr(te, "type"))
        .map(|v| v.to_string());
    out.push(Element::Line(LineElement {
        x, y, w, h, color, width_pt, dash,
        flip_h: geom.flip_h,
        flip_v: geom.flip_v,
        tail_end,
    }));
}

fn read_gframe(node: Node, t: &T, out: &mut Vec<Element>) {
    let xf = child(node, "xfrm");
    let off = xf.and_then(|xf| child(xf, "off"));
    let ext = xf.and_then(|xf| child(xf, "ext"));
    let x = off.and_then(|o| attr(o, "x")).and_then(|v| v.parse().ok()).unwrap_or(0);
    let y = off.and_then(|o| attr(o, "y")).and_then(|v| v.parse().ok()).unwrap_or(0);
    let w = ext.and_then(|e| attr(e, "cx")).and_then(|v| v.parse().ok()).unwrap_or(0);
    let h = ext.and_then(|e| attr(e, "cy")).and_then(|v| v.parse().ok()).unwrap_or(0);

    let is_table = node
        .descendants()
        .any(|d| d.tag_name().name() == "tbl");
    if !is_table {
        let what = node
            .descendants()
            .find(|d| d.tag_name().name() == "graphicData")
            .and_then(|g| attr(g, "uri"))
            .map(|u| {
                if u.contains("chart") {
                    "chart".to_string()
                } else if u.contains("diagram") {
                    "smartart".to_string()
                } else {
                    u.to_string()
                }
            })
            .unwrap_or_else(|| "graphicFrame".to_string());
        out.push(Element::Unsupported(UnsupportedElement { x, y, w, h, what, note: String::new() }));
        return;
    }
    let tbl = find(node, "tbl").expect("tbl");
    let col_widths: Vec<i64> = find(tbl, "tblGrid")
        .map(|g| children(g, "gridCol").iter().filter_map(|c| attr(*c, "w").and_then(|v| v.parse().ok())).collect())
        .unwrap_or_default();
    let mut cells: Vec<Vec<String>> = Vec::new();
    let mut row_heights: Vec<i64> = Vec::new();
    for tr in children(tbl, "tr") {
        row_heights.push(attr(tr, "h").and_then(|v| v.parse().ok()).unwrap_or(0));
        let mut row = Vec::new();
        for tc in children(tr, "tc") {
            row.push(text_of(tc));
        }
        cells.push(row);
    }
    out.push(Element::Table(TableElement { x, y, w, h, col_widths, row_heights, cells }));
}

fn read_grp(node: Node, ctx: &mut Ctx, slide_idx: usize, slide_rels: &HashMap<String, String>, t: &T, out: &mut Vec<Element>) {
    let (grp, chx, chy) = match read_group_xfrm(node) {
        Some(v) => v,
        None => return,
    };
    if grp.rot.is_some() {
        ctx.note(format!("第 {} 页：组合内旋转未合成（子元素按位置拍平）", slide_idx + 1));
    }
    let ch_ext = child(node, "grpSpPr")
        .and_then(|g| child(g, "xfrm"))
        .and_then(|xf| child(xf, "chExt"));
    let chw = ch_ext.as_ref().and_then(|e| attr(*e, "cx")).and_then(|v| v.parse().ok()).unwrap_or(0);
    let chh = ch_ext.as_ref().and_then(|e| attr(*e, "cy")).and_then(|v| v.parse().ok()).unwrap_or(0);
    let child_t = T {
        ox: t.map_x(grp.x),
        oy: t.map_y(grp.y),
        chx,
        chy,
        sx: t.sx * if chw == 0 { 1.0 } else { grp.w as f64 / chw as f64 },
        sy: t.sy * if chh == 0 { 1.0 } else { grp.h as f64 / chh as f64 },
    };
    ctx.note(format!("第 {} 页：组合形状已拍平（位置保留，分组关系丢失）", slide_idx + 1));
    for c in node.children().filter(|n| n.is_element()) {
        let local = c.tag_name().name();
        if matches!(local, "nvGrpSpPr" | "grpSpPr") {
            continue;
        }
        read_any(c, ctx, slide_idx, slide_rels, &child_t, out);
    }
}


/// FNV-1a 64 位内容哈希 —— 同一内容永远得到同一 asset 名
fn fnv1a(bytes: &[u8]) -> String {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in bytes {
        h ^= *b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    format!("{:016x}", h)
}
