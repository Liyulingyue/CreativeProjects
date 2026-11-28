//! PPTX → manifest 读取器。zip + roxmltree，不走 python-pptx。
use super::parts;
use super::theme::Theme;
use crate::error::{BridgeError, Result};
use crate::model::*;
use crate::xmlutil::*;
use roxmltree::Node;
use std::collections::HashMap;
use std::path::Path;

pub struct ReadOutcome {
    pub manifest: Presentation,
    pub fidelity: Vec<String>,
    pub assets: Vec<(String, Vec<u8>)>, // (asset 文件名, 字节)
}

/// slide 级 relationship：target 原文 + 是否外部（URL 等）
#[derive(Clone, Debug)]
struct Rel {
    target: String,
    external: bool,
}

struct Ctx<'a> {
    zip: &'a mut zip::ZipArchive<std::fs::File>,
    theme: Theme,
    /// 保真度消息 → (出现次数, 首次出现序号)：汇总时按首次顺序输出并带 ×N
    fidelity: HashMap<String, (usize, usize)>,
    fidelity_seq: usize,
    assets: Vec<(String, Vec<u8>)>,
    media_done: HashMap<String, String>, // 包内路径 → asset 文件名
    /// slide part 路径 → manifest 页号（按 sldIdLst 顺序，跳页链接以此为准）
    page_of_part: HashMap<String, u32>,
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
        let e = self
            .fidelity
            .entry(msg.into())
            .or_insert((0, self.fidelity_seq));
        e.0 += 1;
        self.fidelity_seq += 1;
    }
    /// 汇总保真度报告：按首次出现顺序，重复消息追加 ×N
    fn fidelity_lines(&self) -> Vec<String> {
        let mut items: Vec<(&String, &(usize, usize))> = self.fidelity.iter().collect();
        items.sort_by_key(|(_, (_, seq))| *seq);
        items
            .into_iter()
            .map(|(msg, (n, _))| if *n > 1 { format!("{msg}（×{n}）") } else { msg.clone() })
            .collect()
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
    /// 读取备注页正文（body 占位符里的段落，\n 连接）
    fn read_notes(&mut self, part: &str) -> Option<String> {
        let xml = self.part_str(part).ok()?;
        let doc = roxmltree::Document::parse(&xml).ok()?;
        let mut paras: Vec<String> = Vec::new();
        for sp in doc.root_element().descendants().filter(|d| d.tag_name().name() == "sp") {
            let ph_type = child(sp, "nvSpPr")
                .and_then(|nv| child(nv, "nvPr"))
                .and_then(|nv| child(nv, "ph"))
                .and_then(|ph| attr(ph, "type"))
                .map(|v| v.to_string());
            match ph_type.as_deref() {
                Some("body") => {
                    if let Some(tb) = child(sp, "txBody") {
                        for p in children(tb, "p") {
                            paras.push(para_text(p));
                        }
                    }
                }
                // sldImg/sldNum/hdr/dt/ftr 等占位一律跳过；无类型的不收
                _ => {}
            }
        }
        Some(paras.join("\n"))
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
    let lenient = super::zip_repair::open_lenient(path)?;
    let mut zip = lenient.archive;
    let mut ctx = Ctx {
        zip: &mut zip,
        theme: Theme::default(),
        fidelity: HashMap::new(),
        fidelity_seq: 0,
        assets: vec![],
        media_done: HashMap::new(),
        page_of_part: HashMap::new(),
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

    // 3. theme + master clrMap（无 theme part 时按内嵌模板缺省，保证往返一致）
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
        None => {
            ctx.note("源文件无 theme part，按缺省主题处理");
            parts::THEME_XML.to_string()
        }
    };
    let master_xml = match &master_part {
        Some(p) => Some(ctx.part_str(p)?),
        None => None,
    };
    ctx.theme = Theme::parse(&theme_xml, master_xml.as_deref());

    // 3.5 母版资源包：master + layouts + raw 主题 + 默认文本样式 + 母版侧媒体
    let mut pack = collect_template_pack(&mut ctx, &pres_xml, master_part.as_deref(), master_xml.as_deref(), &theme_xml)?;
    if pack.master_xml.is_empty() || is_blank_master_stack(&pack) {
        pack = TemplatePack::default(); // 母版栈与内嵌空白模板等价 → 归一为 None（主题走 typed 字段）
    }
    let has_pack = pack != TemplatePack::default();

    // 4. 逐页解析
    let mut slides = Vec::new();
    for (i, rid) in sld_ids.iter().enumerate() {
        let slide_part = rel_map
            .get(rid)
            .ok_or_else(|| BridgeError::Other(format!("slide rId {rid} 无对应 part")))?;
        // 页号以 sldIdLst 顺序为准（文件名编号与页序可能不一致）
        ctx.page_of_part.insert(slide_part.clone(), (i + 1) as u32);
        let slide_base = parent_dir(slide_part);
        let slide_rels_path = format!("{}/_rels/{}.rels", slide_base, file_name(slide_part));

        let mut slide_rels: HashMap<String, Rel> = HashMap::new();
        if ctx.zip.by_name(&slide_rels_path).is_ok() {
            let rels_xml = ctx.part_str(&slide_rels_path)?;
            let rd = roxmltree::Document::parse(&rels_xml)?;
            for r in rd.root_element().children().filter(|n| n.tag_name().name() == "Relationship") {
                if let (Some(id), Some(target)) = (attr(r, "Id"), attr(r, "Target")) {
                    let external = attr(r, "TargetMode").map(|m| m == "External").unwrap_or(false);
                    let target = if external { target.to_string() } else { resolve(slide_base, target) };
                    slide_rels.insert(id.to_string(), Rel { target, external });
                }
            }
        } else {
            ctx.note("有 slide 缺少 rels part");
        }

        let xml = ctx.part_str(slide_part)?;
        let sdoc = roxmltree::Document::parse(&xml)?;
        let root_el = sdoc.root_element();

        // 背景（bg 与 spTree 都在 cSld 里）
        let bg_node = find(root_el, "bg");
        let bg_pr = bg_node.and_then(|bg| child(bg, "bgPr"));
        let background_color = bg_pr
            .and_then(|bgpr| child(bgpr, "solidFill"))
            .and_then(|sf| child(sf, "srgbClr"))
            .and_then(|c| attr(c, "val"))
            .map(|v| v.to_uppercase());
        let background_image = bg_pr
            .and_then(|bgpr| child(bgpr, "blipFill"))
            .and_then(|bf| child(bf, "blip"))
            .and_then(|b| attr_ns(b, NS_R, "embed"))
            .and_then(|rid| slide_rels.get(rid).cloned())
            .map(|rel| ctx.extract_media(rel.target));
        if bg_node.is_some() && bg_pr.is_none() {
            ctx.note("背景使用主题引用（bgRef），仅提取实体背景");
        }

        // spTree
        let t = T::identity();
        let mut elements = Vec::new();
        if let Some(sp_tree) = find(root_el, "spTree") {
            for c in sp_tree.children().filter(|n| n.is_element()) {
                read_any(c, &mut ctx, i, &slide_rels, &t, &mut elements);
            }
        }

        // 备注页正文
        let notes = slide_rels
            .values()
            .find(|r| !r.external && r.target.contains("notesSlide") && r.target.ends_with(".xml"))
            .map(|r| ctx.read_notes(&r.target))
            .unwrap_or(None);

        // 本页版式编号（1 归一为 None，保证手写 manifest 往返一致）
        let layout = slide_rels
            .values()
            .find(|r| !r.external && r.target.contains("slideLayout"))
            .and_then(|r| part_number(&r.target, "slideLayout"))
            .filter(|n| *n != 1);

        slides.push(Slide { elements, background_color, background_image, notes, layout });
    }

    let manifest = Presentation {
        kind: "presentation".into(),
        width_emu,
        height_emu,
        slides,
        theme: Some(ctx.theme.manifest.clone()),
        template: if has_pack { Some(pack) } else { None },
    };

    let mut fidelity = ctx.fidelity_lines();
    if fidelity.is_empty() {
        fidelity.push("转换全程无损".into());
    }
    let outcome = ReadOutcome {
        manifest,
        fidelity,
        assets: ctx.assets,
    };
    // 修复包的临时文件：先关句柄再删（Windows 不允许删除打开中的文件）
    drop(zip);
    if let Some(tmp) = lenient.temp_path {
        let _ = std::fs::remove_file(tmp);
    }
    Ok(outcome)
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
}

fn read_geom_of(node: Node) -> Option<Geom> {
    // xfrm 在 spPr 里（sp/pic/cxnSp）；组合在 grpSpPr 里；graphicFrame 的 p:xfrm 由 read_gframe 单独处理
    let sp_pr = child(node, "spPr").or_else(|| child(node, "grpSpPr"))?;
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
    Some(Geom { x, y, w, h, rot, flip_h, flip_v })
}

// ---------------------------------------------------------------- 元素分发

fn read_any(node: Node, ctx: &mut Ctx, slide_idx: usize, slide_rels: &HashMap<String, Rel>, t: &T, out: &mut Vec<Element>) {
    let local = node.tag_name().name();
    match local {
        "sp" => read_sp(node, ctx, slide_idx, slide_rels, t, out),
        "pic" => read_pic(node, ctx, slide_rels, t, out),
        "cxnSp" => read_cxn(node, ctx, t, out),
        "graphicFrame" => read_gframe(node, ctx, t, out),
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
    let xf = child(node, "grpSpPr").and_then(|g| child(g, "xfrm"))?;
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

/// cNvPr 上的形状名（v0.2 起用于识别写侧的 unsupported 标记）
fn shape_name_of<'a, 'b>(node: Node<'a, 'b>) -> (Option<&'a str>, Option<&'a str>) {
    let nv = child(node, "nvSpPr")
        .and_then(|nv| child(nv, "cNvPr"))
        .or_else(|| node.descendants().find(|d| d.tag_name().name() == "cNvPr"));
    match nv {
        Some(n) => (attr(n, "name"), attr(n, "descr")),
        None => (None, None),
    }
}

fn read_sp(node: Node, ctx: &mut Ctx, slide_idx: usize, slide_rels: &HashMap<String, Rel>, t: &T, out: &mut Vec<Element>) {
    // 占位符可无自身 xfrm（几何继承版式）——不能丢
    let (ph_type, ph_idx) = node
        .descendants()
        .find(|d| d.tag_name().name() == "ph")
        .map(|ph| (attr(ph, "type").map(|v| v.to_string()), attr(ph, "idx").and_then(|v| v.parse().ok())))
        .unwrap_or((None, None));

    let geom = match read_geom_of(node) {
        Some(g) => g,
        None => {
            if ph_type.is_none() {
                return; // 无几何且非占位符：无法定位，跳过
            }
            let paragraphs = child(node, "txBody").map(|tb| read_paragraphs(tb, ctx, slide_rels));
            out.push(Element::Shape(ShapeElement {
                x: 0,
                y: 0,
                w: 0,
                h: 0,
                ph_type,
                ph_idx,
                inherit_geom: true,
                paragraphs,
                ..Default::default()
            }));
            return;
        }
    };
    let (x, y, w, h) = (t.map_x(geom.x), t.map_y(geom.y), t.dim(geom.w, true), t.dim(geom.h, false));

    // ① 写侧标记：unsupported 占位在此还原（往返逐字节稳定的关键）
    let (name, descr) = shape_name_of(node);
    if let Some(what) = name.and_then(|n| n.strip_prefix("phb:unsupported:")) {
        out.push(Element::Unsupported(UnsupportedElement {
            x, y, w, h,
            what: what.to_string(),
            note: descr.unwrap_or("").to_string(),
        }));
        return;
    }

    let sp_pr = child(node, "spPr");
    let prst = sp_pr
        .and_then(|sp| child(sp, "prstGeom"))
        .and_then(|g| attr(g, "prst"))
        .unwrap_or("rect")
        .to_string();
    let cust_geom_node = sp_pr.and_then(|sp| child(sp, "custGeom"));

    let tx_box = node
        .descendants()
        .find(|d| d.tag_name().name() == "cNvSpPr")
        .and_then(|d| attr(d, "txBox"))
        .map(|v| v == "1")
        .unwrap_or(false);

    // 颜色读取
    let fill_color_node = sp_pr.and_then(|sp| child(sp, "solidFill")).and_then(|sf| sf.children().find(|n| n.is_element()));
    let fill_color = fill_color_node.and_then(|c| read_color_node(c, ctx).map(|(col, _)| col));
    let fill_alpha_pct = fill_color_node.and_then(|c| color_alpha(c));
    let gradient = sp_pr.and_then(|sp| read_gradient(sp, ctx));
    let (line_color, line_width_pt, line_dash, head_end, tail_end) = sp_pr
        .and_then(|sp| child(sp, "ln"))
        .map(|ln| {
            let c = child(ln, "solidFill").and_then(|sf| read_color_child(sf, ctx));
            let w = attr(ln, "w").and_then(|v| v.parse::<f64>().ok()).map(|e| e / 12700.0);
            let d = child(ln, "prstDash").and_then(|d| attr(d, "val")).map(|v| v.to_string());
            let he = child(ln, "headEnd").and_then(|d| attr(d, "type")).map(|v| v.to_string());
            let te = child(ln, "tailEnd").and_then(|d| attr(d, "type")).map(|v| v.to_string());
            (c, w, d, he, te)
        })
        .unwrap_or((None, None, None, None, None));
    let shadow = sp_pr.and_then(|sp| child(sp, "effectLst")).and_then(|e| child(e, "outerShdw")).map(read_shadow);

    if sp_pr.and_then(|sp| child(sp, "ln")).and_then(|ln| child(ln, "gradFill")).is_some() {
        ctx.note("线条使用渐变描边，已降级为无色（宽度/箭头保留）");
    }

    // prstGeom 的 adj 调整值（roundRect 圆角等）
    let adj_val = sp_pr
        .and_then(|sp| child(sp, "prstGeom"))
        .and_then(|g| child(g, "avLst"))
        .and_then(|av| child(av, "gd"))
        .and_then(|gd| attr(gd, "fmla"))
        .and_then(|f| f.strip_prefix("val "))
        .and_then(|v| v.parse().ok());

    // 占位符（ph@type / ph@idx）——影响继承与渲染语义
    let (ph_type, ph_idx) = node
        .descendants()
        .find(|d| d.tag_name().name() == "ph")
        .map(|ph| (attr(ph, "type").map(|v| v.to_string()), attr(ph, "idx").and_then(|v| v.parse().ok())))
        .unwrap_or((None, None));

    let paragraphs = child(node, "txBody").map(|tb| read_paragraphs(tb, ctx, slide_rels));

    if prst == "line" {
        out.push(Element::Line(LineElement {
            x, y, w, h,
            color: line_color,
            width_pt: line_width_pt,
            dash: line_dash,
            flip_h: geom.flip_h,
            flip_v: geom.flip_v,
            head_end,
            tail_end,
        }));
        return;
    }

    // ② 自定义几何 → freeform（v0.2 起完整往返）
    if let Some(cg) = cust_geom_node {
        let subpaths = read_cust_geom(cg, ctx, slide_idx);
        let has_fill = fill_color.is_some() || gradient.is_some();
        out.push(Element::Freeform(FreeformElement {
            x, y, w, h,
            path_w: subpaths.first().and_then(|s| s.w),
            path_h: subpaths.first().and_then(|s| s.h),
            subpaths,
            fill_type: Some(if has_fill { "solid".to_string() } else { "none".to_string() }),
            fill_color: fill_color.or_else(|| gradient.as_ref().and_then(|(stops, _)| stops.first().map(|s| s.color.clone()))),
            line_color,
            line_width_pt,
            rotation: geom.rot,
        }));
        if gradient.is_some() {
            ctx.note("自由形状的渐变填充取首个停靠点近似");
        }
        return;
    }

    if tx_box {
        let (fs, lr) = read_auto_fit_scale(node);
        out.push(Element::Text(TextElement {
            x, y, w, h,
            paragraphs: paragraphs.unwrap_or_default(),
            rotation: geom.rot,
            word_wrap: read_word_wrap(node),
            auto_size: read_auto_size(node),
            anchor: read_anchor(node),
            shadow,
            insets: read_insets(node, ctx),
            font_scale_pct: fs,
            ln_spc_reduction_pct: lr,
        }));
        return;
    }

    // 普通形状
    let fill_type = if gradient.is_some() {
        "gradient"
    } else if fill_color.is_some() {
        "solid"
    } else {
        "none"
    };
    if sp_pr.and_then(|sp| child(sp, "blipFill")).is_some() {
        ctx.note("形状的图片填充转为无填充（位置保留）");
    }
    if sp_pr.and_then(|sp| child(sp, "pattFill")).is_some() {
        ctx.note("形状的图案填充转为无填充（位置保留）");
    }
    let (fill_stops, fill_angle_deg) = gradient.map(|(s, a)| (Some(s), a)).unwrap_or((None, None));
    if fill_type == "gradient" && fill_stops.as_ref().map(|s| s.is_empty()).unwrap_or(true) {
        ctx.note("渐变填充无有效停靠点，转为无填充");
    }
    out.push(Element::Shape(ShapeElement {
        x, y, w, h,
        geometry: prst,
        fill_type: Some(fill_type.to_string()),
        fill_color,
        fill_alpha_pct,
        fill_stops,
        fill_angle_deg,
        line_color,
        line_width_pt,
        line_dash,
        rotation: geom.rot,
        shadow,
        paragraphs,
        insets: read_insets(node, ctx),
        anchor: read_shape_anchor(node),
        adj_val,
        ph_type,
        ph_idx,
        inherit_geom: false,
    }));
}

/// custGeom → 子路径列表
fn read_cust_geom(cg: Node, ctx: &mut Ctx, slide_idx: usize) -> Vec<SubPath> {
    let mut out = Vec::new();
    if let Some(lst) = child(cg, "pathLst") {
        for p in children(lst, "path") {
            let pt_of = |c: Node| -> Option<(i64, i64)> {
                let pt = child(c, "pt")?;
                Some((attr(pt, "x").and_then(|v| v.parse().ok())?, attr(pt, "y").and_then(|v| v.parse().ok())?))
            };
            let mut cmds = Vec::new();
            let mut closed = false;
            for c in p.children().filter(|n| n.is_element()) {
                match c.tag_name().name() {
                    "moveTo" => {
                        if let Some((x, y)) = pt_of(c) {
                            cmds.push(PathCmd::Move { x, y });
                        }
                    }
                    "lnTo" => {
                        if let Some((x, y)) = pt_of(c) {
                            cmds.push(PathCmd::Line { x, y });
                        }
                    }
                    "cubicBezTo" => {
                        let pts: Vec<(i64, i64)> = children(c, "pt")
                            .iter()
                            .filter_map(|pt| Some((attr(*pt, "x").and_then(|v| v.parse().ok())?, attr(*pt, "y").and_then(|v| v.parse().ok())?)))
                            .collect();
                        if pts.len() == 3 {
                            cmds.push(PathCmd::Cubic { x1: pts[0].0, y1: pts[0].1, x2: pts[1].0, y2: pts[1].1, x: pts[2].0, y: pts[2].1 });
                        }
                    }
                    "quadBezTo" => {
                        let pts: Vec<(i64, i64)> = children(c, "pt")
                            .iter()
                            .filter_map(|pt| Some((attr(*pt, "x").and_then(|v| v.parse().ok())?, attr(*pt, "y").and_then(|v| v.parse().ok())?)))
                            .collect();
                        if pts.len() == 2 {
                            cmds.push(PathCmd::Quad { x1: pts[0].0, y1: pts[0].1, x: pts[1].0, y: pts[1].1 });
                        }
                    }
                    "arcTo" => {
                        let num = |a: &str| attr(c, a).and_then(|v| v.parse::<f64>().ok());
                        if let (Some(wr), Some(hr)) = (num("wR"), num("hR")) {
                            cmds.push(PathCmd::Arc {
                                w_r: wr as i64,
                                h_r: hr as i64,
                                st_ang_deg: num("stAng").unwrap_or(0.0) / 60000.0,
                                sw_ang_deg: num("swAng").unwrap_or(0.0) / 60000.0,
                            });
                        }
                    }
                    "close" => closed = true,
                    _ => {}
                }
            }
            if attr(p, "fill").map(|v| v != "norm").unwrap_or(false) {
                ctx.note(format!("第 {} 页：子路径的特殊填充标记（fill）被忽略", slide_idx + 1));
            }
            out.push(SubPath {
                w: attr(p, "w").and_then(|v| v.parse().ok()),
                h: attr(p, "h").and_then(|v| v.parse().ok()),
                closed,
                cmds,
            });
        }
    }
    out
}

/// 渐变填充 → (停靠点, 角度)
fn read_gradient(sp: Node, ctx: &mut Ctx) -> Option<(Vec<GradientStop>, Option<f64>)> {
    let gf = child(sp, "gradFill")?;
    let mut stops = Vec::new();
    if let Some(gsl) = child(gf, "gsLst") {
        for gs in children(gsl, "gs") {
            let pos_pct = attr(gs, "pos").and_then(|v| v.parse::<f64>().ok()).map(|v| v / 1000.0);
            let c = gs.children().find(|n| n.is_element());
            if let (Some(pos), Some(c)) = (pos_pct, c) {
                if let Some((color, alpha)) = read_color_node(c, ctx) {
                    stops.push(GradientStop { pos_pct: pos, color, alpha_pct: alpha });
                }
            }
        }
    }
    if child(gf, "path").is_some() {
        ctx.note("路径渐变（path）按线性渐变近似");
    }
    let angle = child(gf, "lin")
        .and_then(|l| attr(l, "ang"))
        .and_then(|v| v.parse::<f64>().ok())
        .map(|v| v / 60000.0);
    Some((stops, angle))
}

/// 颜色节点 → (RRGGBB, alpha%)。srgbClr/sysClr/schemeClr 通用，带 alpha 子元素。
fn read_color_node(c: Node, ctx: &mut Ctx) -> Option<(String, Option<f64>)> {
    let hex = match c.tag_name().name() {
        "srgbClr" => attr(c, "val").map(|v| v.to_uppercase()),
        "sysClr" => attr(c, "lastClr").map(|v| v.to_uppercase()),
        "schemeClr" => {
            ctx.note("theme/scheme 颜色已折算为 RGB");
            ctx.theme.resolve_scheme_node(c)
        }
        _ => None,
    }?;
    let alpha = color_alpha(c);
    Some((hex, alpha))
}

/// 颜色节点的 alpha 子元素 → 0-100
fn color_alpha(c: Node) -> Option<f64> {
    child(c, "alpha")
        .and_then(|a| attr(a, "val"))
        .and_then(|v| v.parse::<f64>().ok())
        .map(|v| v / 1000.0)
}

fn read_color_child(fill_node: Node, ctx: &mut Ctx) -> Option<String> {
    let c = fill_node.children().find(|n| n.is_element())?;
    read_color_node(c, ctx).map(|(col, _)| col)
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
        algn: attr(n, "algn").map(|v| v.to_string()),
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

/// bodyPr 的四边内边距 EMU [l, t, r, b]。四边齐全才收（部分设置会把
/// 缺失槽位误存成 0），部分出现的记保真提示并按缺省处理。
fn read_insets(sp_node: Node, ctx: &mut Ctx) -> Option<[i64; 4]> {
    let bp = find(sp_node, "bodyPr")?;
    let get = |a: &str| attr(bp, a).and_then(|v| v.parse().ok());
    let ins = [get("lIns"), get("tIns"), get("rIns"), get("bIns")];
    let some_count = ins.iter().filter(|v| v.is_some()).count();
    if some_count == 4 {
        Some(ins.map(|v| v.unwrap_or(0)))
    } else {
        if some_count > 0 {
            ctx.note("bodyPr 仅部分内边距被设置，按缺省边距处理");
        }
        None
    }
}

/// normAutofit 的 fontScale / lnSpcReduction（千分比 → 百分比）
fn read_auto_fit_scale(sp_node: Node) -> (Option<f64>, Option<f64>) {
    let bp = find(sp_node, "bodyPr");
    let nf = bp.and_then(|b| child(b, "normAutofit"));
    let f = |a: &str| {
        nf.and_then(|n| attr(n, a))
            .and_then(|v| v.parse::<f64>().ok())
            .map(|v| v / 1000.0)
    };
    (f("fontScale"), f("lnSpcReduction"))
}

fn read_anchor(sp_node: Node) -> Option<String> {
    let bp = find(sp_node, "bodyPr")?;
    attr(bp, "anchor").map(|v| match v {
        "t" => "top".to_string(),
        "b" => "bottom".to_string(),
        _ => "middle".to_string(),
    })
}

/// 形状用的 anchor 读取：ctr 归一为 None（写侧形状缺省 ctr），t/b 保留
fn read_shape_anchor(sp_node: Node) -> Option<String> {
    let bp = find(sp_node, "bodyPr")?;
    match attr(bp, "anchor") {
        Some("t") => Some("top".to_string()),
        Some("b") => Some("bottom".to_string()),
        _ => None,
    }
}

/// hlinkClick 节点 → Link（外部 URL 或内部跳页）
/// 内部跳页记 manifest 页号（sldIdLst 顺序），而非 part 文件名编号
fn link_of(hlink: Node, slide_rels: &HashMap<String, Rel>, ctx: &Ctx) -> Option<Link> {
    let rid = attr_ns(hlink, NS_R, "id")?;
    let rel = slide_rels.get(rid)?;
    let tooltip = attr(hlink, "tooltip").map(|v| v.to_string());
    if rel.external {
        Some(Link { url: Some(rel.target.clone()), slide: None, tooltip })
    } else {
        let slide = ctx.page_of_part.get(&rel.target).copied().or_else(|| {
            // 兜底：目标不在已知页表里（罕见），退回文件名编号
            file_name(&rel.target)
                .trim_end_matches(".xml")
                .strip_prefix("slide")
                .and_then(|s| s.parse().ok())
        });
        Some(Link { url: None, slide, tooltip })
    }
}

fn read_paragraphs(tx_body: Node, ctx: &mut Ctx, slide_rels: &HashMap<String, Rel>) -> Vec<Paragraph> {
    let mut out = Vec::new();
    for p in children(tx_body, "p") {
        let mut runs = Vec::new();
        // a:r / a:br / a:fld 都可能携带文字；br 记为 "\n" 文本段（样式照读，保证往返一致）
        for r in p.children().filter(|n| n.is_element()) {
            let tag = r.tag_name().name();
            if !matches!(tag, "r" | "fld" | "br") {
                continue;
            }
            let rpr = child(r, "rPr");
            let text = if tag == "br" {
                "\n".to_string()
            } else {
                child(r, "t").and_then(|t| t.text()).unwrap_or("").to_string()
            };
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
            let latin_el = rpr.and_then(|rp| child(rp, "latin"));
            let font_pitch_family = latin_el.and_then(|l| attr(l, "pitchFamily")).and_then(|v| v.parse().ok());
            let font_charset = latin_el.and_then(|l| attr(l, "charset")).and_then(|v| v.parse().ok());
            let font_charset_ea = rpr
                .and_then(|rp| child(rp, "ea"))
                .and_then(|l| attr(l, "charset"))
                .and_then(|v| v.parse().ok());
            let font_panose = latin_el.and_then(|l| attr(l, "panose")).map(|v| v.to_string());
            let font_sym = rpr.and_then(|rp| child(rp, "sym")).and_then(|l| attr(l, "typeface")).map(|v| v.to_string());
            let font_cs = rpr.and_then(|rp| child(rp, "cs")).and_then(|l| attr(l, "typeface")).map(|v| v.to_string());
            let bold = rpr.and_then(|rp| attr(rp, "b")).map(|v| v == "1");
            let italic = rpr.and_then(|rp| attr(rp, "i")).map(|v| v == "1");
            let underline = rpr.and_then(|rp| attr(rp, "u")).map(|v| v != "none");
            let kern = rpr.and_then(|rp| attr(rp, "kern")).and_then(|v| v.parse().ok());
            let spc = rpr.and_then(|rp| attr(rp, "spc")).and_then(|v| v.parse().ok());
            // 语言：zh-CN 归一为 None（写侧缺省 zh-CN），往返一致
            let lang = rpr
                .and_then(|rp| attr(rp, "lang"))
                .map(|v| v.to_string())
                .filter(|v| v != "zh-CN");
            let mut run = TextRun {
                text,
                bold,
                italic,
                underline,
                size_pt,
                color,
                font,
                font_pitch_family,
                font_charset,
                font_charset_ea,
                font_panose,
                font_sym,
                font_cs,
                link: None,
                kern,
                spc,
                lang,
            };
            if let Some(hl) = rpr.and_then(|rp| child(rp, "hlinkClick")) {
                run.link = link_of(hl, slide_rels, ctx);
            }
            runs.push(run);
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
        let mar_l = ppr.and_then(|pp| attr(pp, "marL")).and_then(|v| v.parse().ok());
        let indent = ppr.and_then(|pp| attr(pp, "indent")).and_then(|v| v.parse().ok());
        let spc = |tag: &str| {
            ppr.and_then(|pp| child(pp, tag))
                .and_then(|s| child(s, "spcPts"))
                .and_then(|s| attr(s, "val"))
                .and_then(|v| v.parse::<f64>().ok())
                .map(|v| v / 100.0)
        };
        let space_before_pt = spc("spcBef");
        let space_after_pt = spc("spcAft");
        // 项目符号
        let mut bullet: Option<Bullet> = None;
        if let Some(pp) = ppr {
            if child(pp, "buNone").is_some() {
                bullet = Some(Bullet { kind: "none".into(), ..Default::default() });
            } else if let Some(bc) = child(pp, "buChar") {
                bullet = Some(Bullet {
                    kind: "char".into(),
                    character: attr(bc, "char").map(|v| v.to_string()),
                    ..Default::default()
                });
            } else if let Some(an) = child(pp, "buAutoNum") {
                bullet = Some(Bullet {
                    kind: "auto_num".into(),
                    num_type: attr(an, "type").map(|v| v.to_string()),
                    num_start: attr(an, "startAt").and_then(|v| v.parse().ok()),
                    ..Default::default()
                });
            }
            if let Some(b) = bullet.as_mut() {
                if let Some(bc) = child(pp, "buClr") {
                    b.color = read_color_child(bc, ctx);
                }
                if let Some(bs) = child(pp, "buSzPct").and_then(|s| attr(s, "val")).and_then(|v| v.parse::<f64>().ok()) {
                    b.size_pct = Some(bs / 1000.0);
                }
                if let Some(bf) = child(pp, "buFont").and_then(|f| attr(f, "typeface")) {
                    b.font = Some(bf.to_string());
                }
            }
        }
        out.push(Paragraph {
            runs,
            align,
            level,
            line_spacing,
            bullet,
            mar_l,
            indent,
            space_before_pt,
            space_after_pt,
        });
    }
    out
}

/// 一段的纯文本（a:r/a:fld 的 t 拼接，a:br 记 \n）
fn para_text(p: Node) -> String {
    let mut s = String::new();
    for r in p.children().filter(|n| n.is_element()) {
        match r.tag_name().name() {
            "r" | "fld" => s.push_str(child(r, "t").and_then(|t| t.text()).unwrap_or("")),
            "br" => s.push('\n'),
            _ => {}
        }
    }
    s
}

fn read_pic(node: Node, ctx: &mut Ctx, slide_rels: &HashMap<String, Rel>, t: &T, out: &mut Vec<Element>) {
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
        .map(|rel| ctx.extract_media(rel.target))
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

    let link = node
        .descendants()
        .find(|d| d.tag_name().name() == "cNvPr")
        .and_then(|c| child(c, "hlinkClick"))
        .and_then(|hl| link_of(hl, slide_rels, ctx));

    out.push(Element::Image(ImageElement {
        x, y, w, h,
        asset,
        rotation: geom.rot,
        crop_pct: crop_pct.filter(|c| c.iter().any(|v| *v != 0.0)),
        link,
    }));
}

fn read_cxn(node: Node, ctx: &mut Ctx, t: &T, out: &mut Vec<Element>) {
    let geom = match read_geom_of(node) {
        Some(g) => g,
        None => return,
    };
    let (x, y, w, h) = (t.map_x(geom.x), t.map_y(geom.y), t.dim(geom.w, true), t.dim(geom.h, false));
    let ln_node = child(node, "spPr").and_then(|sp| child(sp, "ln"));
    if ln_node.and_then(|ln| child(ln, "gradFill")).is_some() {
        ctx.note("线条使用渐变描边，已降级为无色（宽度/箭头保留）");
    }
    let (color, width_pt, dash, head_end, tail_end) = ln_node
        .map(|ln| {
            let c = child(ln, "solidFill").and_then(|sf| read_color_child(sf, ctx));
            let w = attr(ln, "w").and_then(|v| v.parse::<f64>().ok()).map(|e| e / 12700.0);
            let d = child(ln, "prstDash").and_then(|d| attr(d, "val")).map(|v| v.to_string());
            let he = child(ln, "headEnd").and_then(|d| attr(d, "type")).map(|v| v.to_string());
            let te = child(ln, "tailEnd").and_then(|d| attr(d, "type")).map(|v| v.to_string());
            (c, w, d, he, te)
        })
        .unwrap_or((None, None, None, None, None));
    out.push(Element::Line(LineElement {
        x, y, w, h, color, width_pt, dash,
        flip_h: geom.flip_h,
        flip_v: geom.flip_v,
        head_end,
        tail_end,
    }));
}

fn read_gframe(node: Node, ctx: &mut Ctx, t: &T, out: &mut Vec<Element>) {
    let xf = child(node, "xfrm");
    let off = xf.and_then(|xf| child(xf, "off"));
    let ext = xf.and_then(|xf| child(xf, "ext"));
    let rx = off.and_then(|o| attr(o, "x")).and_then(|v| v.parse().ok()).unwrap_or(0);
    let ry = off.and_then(|o| attr(o, "y")).and_then(|v| v.parse().ok()).unwrap_or(0);
    let rw = ext.and_then(|e| attr(e, "cx")).and_then(|v| v.parse().ok()).unwrap_or(0);
    let rh = ext.and_then(|e| attr(e, "cy")).and_then(|v| v.parse().ok()).unwrap_or(0);
    // 组合内的表格/图表同样要拍平映射
    let (x, y, w, h) = (t.map_x(rx), t.map_y(ry), t.dim(rw, true), t.dim(rh, false));

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
    let mut cells: Vec<Vec<TableCell>> = Vec::new();
    let mut row_heights: Vec<i64> = Vec::new();
    for tr in children(tbl, "tr") {
        row_heights.push(attr(tr, "h").and_then(|v| v.parse().ok()).unwrap_or(0));
        let mut row = Vec::new();
        for tc in children(tr, "tc") {
            let fill_color = child(tc, "tcPr")
                .and_then(|tp| child(tp, "solidFill"))
                .and_then(|sf| read_color_child(sf, ctx));
            let col_span = attr(tc, "gridSpan").and_then(|v| v.parse().ok());
            let row_span = attr(tc, "rowSpan").and_then(|v| v.parse().ok());
            let merged_away = attr(tc, "hMerge").map(|v| v == "1").unwrap_or(false)
                || attr(tc, "vMerge").map(|v| v == "1").unwrap_or(false);
            // 文本 = 各段 \n 连接；样式取首段首 run
            let paras = child(tc, "txBody").map(|tb| children(tb, "p")).unwrap_or_default();
            let text = paras
                .iter()
                .map(|p| para_text(*p))
                .collect::<Vec<_>>()
                .join("\n");
            let first_p = paras.first().copied();
            let align = first_p
                .and_then(|p| child(p, "pPr"))
                .and_then(|pp| attr(pp, "algn"))
                .map(|v| match v {
                    "ctr" => "center".to_string(),
                    "l" => "left".to_string(),
                    "r" => "right".to_string(),
                    other => other.to_string(),
                });
            let first_run = first_p.and_then(|p| children(p, "r").first().copied());
            let rpr = first_run.and_then(|r| child(r, "rPr"));
            let bold = rpr.and_then(|rp| attr(rp, "b")).map(|v| v == "1");
            let size_pt = rpr
                .and_then(|rp| attr(rp, "sz"))
                .and_then(|v| v.parse::<f64>().ok())
                .map(|v| v / 100.0);
            let color = rpr
                .and_then(|rp| child(rp, "solidFill"))
                .and_then(|sf| read_color_child(sf, ctx));
            row.push(TableCell { text, fill_color, col_span, row_span, merged_away, bold, color, size_pt, align });
            // gridSpan 隐含的横向延续格补齐（保持网格列位）
            if let Some(cs) = col_span {
                for _ in 1..cs {
                    row.push(TableCell { text: String::new(), merged_away: true, ..Default::default() });
                }
            }
        }
        cells.push(row);
    }
    out.push(Element::Table(TableElement { x, y, w, h, col_widths, row_heights, cells }));
}

fn read_grp(node: Node, ctx: &mut Ctx, slide_idx: usize, slide_rels: &HashMap<String, Rel>, t: &T, out: &mut Vec<Element>) {
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

// ---------------------------------------------------------------- 母版资源包

/// 收集源文件的母版资源包（master / layouts / raw 主题 / 默认文本样式 / 母版侧媒体）
fn collect_template_pack(
    ctx: &mut Ctx,
    pres_xml: &str,
    master_part: Option<&str>,
    master_xml: Option<&str>,
    theme_xml: &str,
) -> Result<TemplatePack> {
    let mut pack = TemplatePack {
        theme_xml: theme_xml.to_string(),
        ..Default::default()
    };
    let Some(mp) = master_part else { return Ok(pack) };
    let mbase = parent_dir(mp);
    pack.master_xml = master_xml.unwrap_or_default().to_string();

    // master rels（原样保留，写侧直接回写）
    let mrels_path = format!("{}/_rels/{}.rels", mbase, file_name(mp));
    if ctx.zip.by_name(&mrels_path).is_ok() {
        pack.master_rels_xml = ctx.part_str(&mrels_path)?;
    }
    let rels_doc = roxmltree::Document::parse(&pack.master_rels_xml)?;
    let mut layout_targets: Vec<(u32, String)> = Vec::new();
    for r in rels_doc.root_element().children().filter(|n| n.tag_name().name() == "Relationship") {
        let (Some(ty), Some(target)) = (attr(r, "Type"), attr(r, "Target")) else { continue };
        let resolved = if attr(r, "TargetMode").map(|m| m == "External").unwrap_or(false) {
            continue;
        } else {
            resolve(mbase, target)
        };
        if let Some(n) = part_number(&resolved, "slideLayout") {
            layout_targets.push((n, resolved));
        } else if ty.ends_with("/image") {
            let asset = ctx.extract_media(resolved.clone());
            if !pack.media.iter().any(|m| m.path == resolved) {
                pack.media.push(MediaRef { path: resolved, asset });
            }
        }
    }
    // 每个版式：xml + rels + 版式侧媒体（按编号去重，避免写出重复 part）
    layout_targets.sort_by_key(|(n, _)| *n);
    layout_targets.dedup_by(|a, b| a.0 == b.0);
    for (n, lp) in layout_targets {
        if pack.layouts.iter().any(|l| l.number == n) {
            continue;
        }
        let lbase = parent_dir(&lp);
        let xml = ctx.part_str(&lp)?;
        let lrels_path = format!("{}/_rels/{}.rels", lbase, file_name(&lp));
        let rels_xml = if ctx.zip.by_name(&lrels_path).is_ok() {
            ctx.part_str(&lrels_path)?
        } else {
            String::new()
        };
        if !rels_xml.is_empty() {
            let ld = roxmltree::Document::parse(&rels_xml)?;
            for r in ld.root_element().children().filter(|n| n.tag_name().name() == "Relationship") {
                let (Some(ty), Some(target)) = (attr(r, "Type"), attr(r, "Target")) else { continue };
                if ty.ends_with("/image")
                    && !attr(r, "TargetMode").map(|m| m == "External").unwrap_or(false)
                {
                    let resolved = resolve(lbase, target);
                    let asset = ctx.extract_media(resolved.clone());
                    if !pack.media.iter().any(|m| m.path == resolved) {
                        pack.media.push(MediaRef { path: resolved, asset });
                    }
                }
            }
        }
        pack.layouts.push(LayoutPart { number: n, xml, rels_xml });
    }
    // 原 presentation.xml 的默认文本样式块
    pack.default_text_style = extract_block(pres_xml, "defaultTextStyle");
    Ok(pack)
}

/// 母版栈是否与内嵌空白模板一致（theme_xml 除外：phb 自己的写出会按 manifest 主题补丁）。
/// 一致 → 读侧归一为 None，保证手写 manifest 往返字节稳定。
fn is_blank_master_stack(pack: &TemplatePack) -> bool {
    let blank = blank_pack();
    pack.master_xml == blank.master_xml
        && pack.master_rels_xml == blank.master_rels_xml
        && pack.media.is_empty()
        && pack.default_text_style == blank.default_text_style
        && pack.layouts.len() == blank.layouts.len()
        && pack.layouts.iter().zip(&blank.layouts).all(|(a, b)| a.number == b.number && a.xml == b.xml && a.rels_xml == b.rels_xml)
}

/// 内嵌空白模板对应的基准包（读侧归一化基准：等于它 → None）
pub fn blank_pack() -> TemplatePack {
    TemplatePack {
        theme_xml: parts::THEME_XML.to_string(),
        master_xml: parts::MASTER_XML.to_string(),
        master_rels_xml: parts::MASTER_RELS.to_string(),
        layouts: (0..parts::LAYOUT_XML.len())
            .map(|i| LayoutPart {
                number: (i + 1) as u32,
                xml: parts::LAYOUT_XML[i].to_string(),
                rels_xml: parts::LAYOUT_RELS[i].to_string(),
            })
            .collect(),
        default_text_style: extract_block(parts::PRES_XML, "defaultTextStyle"),
        media: vec![],
    }
}

/// part 路径 → 编号（slideLayout12.xml → 12）
fn part_number(part: &str, prefix: &str) -> Option<u32> {
    file_name(part)
        .strip_prefix(prefix)?
        .strip_suffix(".xml")?
        .parse()
        .ok()
}

/// 提取 XML 里某个元素的完整块（含开闭标签，容忍命名空间前缀）
fn extract_block(xml: &str, tag: &str) -> Option<String> {
    // 找到元素名出现且前面是 "<ns:" 或 "<" 的位置
    let mut from = 0;
    let (start, name_end) = loop {
        let i = xml[from..].find(tag)? + from;
        let lt = xml[..i].rfind('<')?;
        let between = &xml[lt + 1..i];
        // 允许 "<tag"（无前缀）与 "<ns:tag"（有前缀）
        let is_name = between.chars().all(|c| c == ':' || c.is_alphanumeric() || c == '_' || c == '-');
        if is_name {
            break (lt, i + tag.len());
        }
        from = i + tag.len();
    };
    let open_end = start + xml[start..].find('>')? + 1;
    // 自闭合（<p:tag .../>）直接返回开标签
    if &xml[open_end - 2..open_end] == "/>" {
        return Some(xml[start..open_end].to_string());
    }
    let name = xml[start + 1..name_end].trim();
    let close = format!("</{name}>");
    let end = xml[open_end..].find(&close)? + open_end + close.len();
    Some(xml[start..end].to_string())
}

/// FNV-1a 64 位内容哈希 —— 同一内容永远得到同一 asset 名
fn fnv1a(bytes: &[u8]) -> String {    let mut h: u64 = 0xcbf29ce484222325;
    for b in bytes {
        h ^= *b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    format!("{:016x}", h)
}
