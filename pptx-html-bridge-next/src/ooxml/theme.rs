//! 主题：clrScheme + clrMap，以及 schemeClr → RGB 的解析（含 lumMod/lumOff）。
//! 同时把 clrScheme / fontScheme 提炼为 ThemeManifest（写侧回填用）。
use crate::model::ThemeManifest;
use crate::xmlutil::{attr, child};
use roxmltree::Node;
use std::collections::HashMap;

#[derive(Default, Debug, Clone)]
pub struct Theme {
    /// clrScheme：名称（accent1/dk1/lt1...）→ RRGGBB
    pub scheme: HashMap<String, String>,
    /// clrMap：槽位（tx1/bg1/tx2/bg2）→ scheme 名称
    pub slot_map: HashMap<String, String>,
    /// 可回填的主题清单（读侧提炼）
    pub manifest: ThemeManifest,
}

impl Theme {
    pub fn parse(theme_xml: &str, master_xml: Option<&str>) -> Theme {
        let mut t = Theme::default();
        if let Ok(doc) = roxmltree::Document::parse(theme_xml) {
            let root = doc.root_element();
            if let Some(scheme) = find(root, "clrScheme") {
                t.manifest.name = attr(scheme, "name").map(|v| v.to_string());
                for c in scheme.children().filter(|n| n.is_element()) {
                    let name = c.tag_name().name().to_string();
                    // sysClr 的 val 是系统色名（window/windowText），RGB 必须取 lastClr
                    let hex = if let Some(clr) = child(c, "srgbClr") {
                        attr(clr, "val")
                    } else if let Some(clr) = child(c, "sysClr") {
                        attr(clr, "lastClr")
                    } else {
                        None
                    }
                    .map(|v| v.trim_start_matches('#').to_uppercase());
                    if let Some(hex) = hex {
                        if hex.len() == 6 {
                            t.scheme.insert(name.clone(), hex.clone());
                            match name.as_str() {
                                "dk1" => t.manifest.colors.dk1 = Some(hex),
                                "lt1" => t.manifest.colors.lt1 = Some(hex),
                                "dk2" => t.manifest.colors.dk2 = Some(hex),
                                "lt2" => t.manifest.colors.lt2 = Some(hex),
                                "accent1" => t.manifest.colors.accent1 = Some(hex),
                                "accent2" => t.manifest.colors.accent2 = Some(hex),
                                "accent3" => t.manifest.colors.accent3 = Some(hex),
                                "accent4" => t.manifest.colors.accent4 = Some(hex),
                                "accent5" => t.manifest.colors.accent5 = Some(hex),
                                "accent6" => t.manifest.colors.accent6 = Some(hex),
                                "hlink" => t.manifest.colors.hlink = Some(hex),
                                "folHlink" => t.manifest.colors.fol_hlink = Some(hex),
                                _ => {}
                            }
                        }
                    }
                }
            }
            // 字体方案：latin typeface
            if let Some(major) = find(root, "majorFont") {
                t.manifest.major_font = child(major, "latin").and_then(|l| attr(l, "typeface")).map(|v| v.to_string());
            }
            if let Some(minor) = find(root, "minorFont") {
                t.manifest.minor_font = child(minor, "latin").and_then(|l| attr(l, "typeface")).map(|v| v.to_string());
            }
        }
        if let Some(master) = master_xml {
            if let Ok(doc) = roxmltree::Document::parse(master) {
                if let Some(cm) = find(doc.root_element(), "clrMap") {
                    for a in cm.attributes() {
                        t.slot_map.insert(a.name().to_string(), a.value().to_string());
                    }
                }
            }
        }
        t
    }

    /// 解析 schemeClr 节点 → RGB。lumMod/lumOff 按 ECMA 语义：L' = L × lumMod + lumOff。
    pub fn resolve_scheme_node(&self, node: Node) -> Option<String> {
        let val = attr(node, "val")?;
        let mapped = self.slot_map.get(val).cloned().unwrap_or_else(|| val.to_string());
        let mut rgb = self
            .scheme
            .get(&mapped)
            .cloned()
            .unwrap_or_else(|| mapped.clone());
        let mut lum_mod: Option<f64> = None;
        let mut lum_off: Option<f64> = None;
        for c in node.children().filter(|n| n.is_element()) {
            let v = attr(c, "val").and_then(|v| v.parse::<f64>().ok());
            match c.tag_name().name() {
                "lumMod" => lum_mod = v,
                "lumOff" => lum_off = v,
                _ => {}
            }
        }
        if lum_mod.is_some() || lum_off.is_some() {
            rgb = shift_lum(&rgb, lum_mod, lum_off);
        }
        Some(rgb)
    }
}

fn find<'a, 'b>(n: Node<'a, 'b>, local: &str) -> Option<Node<'a, 'b>> {
    n.descendants().find(|c| c.tag_name().name() == local)
}

fn hex2rgb(h: &str) -> (f64, f64, f64) {
    let v = u32::from_str_radix(h, 16).unwrap_or(0);
    ((v >> 16) as f64, ((v >> 8) & 0xFF) as f64, (v & 0xFF) as f64)
}

fn rgb2hex(r: f64, g: f64, b: f64) -> String {
    let f = |x: f64| x.round().clamp(0.0, 255.0) as u8;
    format!("{:02X}{:02X}{:02X}", f(r), f(g), f(b))
}

/// l（0-1）线性亮度通道
fn lum_of(r: f64, g: f64, b: f64) -> f64 {
    (0.2126 * r + 0.7152 * g + 0.0722 * b) / 255.0
}

fn set_lum(r: f64, g: f64, b: f64, l_new: f64) -> (f64, f64, f64) {
    // 用 HSL 的 L 通道做近似（视觉上足够）
    let (h, s, _) = {
        let r = r / 255.0;
        let g = g / 255.0;
        let b = b / 255.0;
        let max = r.max(g).max(b);
        let min = r.min(g).min(b);
        let l = (max + min) / 2.0;
        let d = max - min;
        let s = if d == 0.0 { 0.0 } else { d / (1.0 - (2.0 * l - 1.0).abs()) };
        let h = if d == 0.0 {
            0.0
        } else if max == r {
            60.0 * (((g - b) / d) % 6.0)
        } else if max == g {
            60.0 * ((b - r) / d + 2.0)
        } else {
            60.0 * ((r - g) / d + 4.0)
        };
        (h / 360.0, s, l)
    };
    let l = l_new.clamp(0.0, 1.0);
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let hp = h * 360.0;
    let x = c * (1.0 - ((hp / 60.0) % 2.0 - 1.0).abs());
    let m = l - c / 2.0;
    let (r1, g1, b1) = match ((hp / 60.0).floor() as i64) % 6 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    ((r1 + m) * 255.0, (g1 + m) * 255.0, (b1 + m) * 255.0)
}

/// PowerPoint lumMod/lumOff 语义：L' = L × lumMod + lumOff（HSL L 通道近似）
fn shift_lum(hex: &str, lum_mod: Option<f64>, lum_off: Option<f64>) -> String {
    let (r, g, b) = hex2rgb(hex);
    let l = lum_of(r, g, b);
    let mut l_new = l;
    if let Some(m) = lum_mod {
        l_new = l * (m / 100000.0);
    }
    if let Some(o) = lum_off {
        l_new += o / 100000.0;
    }
    let (r, g, b) = set_lum(r, g, b, l_new);
    rgb2hex(r, g, b)
}
