use serde::{Deserialize, Serialize};
use thiserror::Error;

/// manifest 的类型化模型 —— 唯一信源。
/// 所有几何量一律 EMU（i64），颜色一律 RRGGBB。
/// `deny_unknown_fields`：字段拼错在反序列化时立即报错并给出路径。
///
/// Element 用手工实现的反序列化：先取 "kind" 再分发到具体结构，
/// 结构体上保留 deny_unknown_fields 的严格性。

#[derive(Error, Debug)]
pub enum ManifestError {
    #[error("{path}: {message}")]
    At { path: String, message: String },
}

impl ManifestError {
    pub fn at(path: &str, message: impl Into<String>) -> Self {
        Self::At { path: path.to_string(), message: message.into() }
    }
}

// ---------------------------------------------------------------- 元素

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(deny_unknown_fields)]
pub struct TextRun {
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bold: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub italic: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub underline: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size_pt: Option<f64>,
    /// RRGGBB
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    /// 同时写入 latin 与 east-asian typeface
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub font: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(deny_unknown_fields)]
pub struct Paragraph {
    #[serde(default)]
    pub runs: Vec<TextRun>,
    /// left | center | right | justify
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub align: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub level: Option<i64>,
    /// 1.5 = 150% 行距
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line_spacing: Option<f64>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Shadow {
    pub blur_pt: f64,
    pub dist_pt: f64,
    /// 度（0 = 从正右方向内）
    pub dir_deg: f64,
    /// RRGGBB
    pub color: String,
    /// 0-100，None = 不透明
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alpha_pct: Option<f64>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct TextElement {
    pub x: i64,
    pub y: i64,
    pub w: i64,
    pub h: i64,
    #[serde(default)]
    pub paragraphs: Vec<Paragraph>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rotation: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub word_wrap: Option<bool>,
    /// none | shape | text
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auto_size: Option<String>,
    /// top | middle | bottom
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub anchor: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shadow: Option<Shadow>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ShapeElement {
    pub x: i64,
    pub y: i64,
    pub w: i64,
    pub h: i64,
    /// MSO_AUTO_SHAPE_TYPE 名的小写：rect / round_rect / ellipse / ...
    #[serde(default = "default_geometry")]
    pub geometry: String,
    /// solid | none
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fill_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fill_color: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line_color: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line_width_pt: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line_dash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rotation: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shadow: Option<Shadow>,
    /// 形状内文字
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub paragraphs: Option<Vec<Paragraph>>,
}

fn default_geometry() -> String {
    "rect".to_string()
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ImageElement {
    pub x: i64,
    pub y: i64,
    pub w: i64,
    pub h: i64,
    /// 相对 assets 目录的文件名
    pub asset: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rotation: Option<f64>,
    /// srcRect 百分比裁切 [l, t, r, b]（0-100）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub crop_pct: Option<[f64; 4]>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct LineElement {
    pub x: i64,
    pub y: i64,
    pub w: i64,
    pub h: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width_pt: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dash: Option<String>,
    #[serde(default)]
    pub flip_h: bool,
    #[serde(default)]
    pub flip_v: bool,
    /// 箭头等尾端样式：triangle / arrow / none ...
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tail_end: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct TableElement {
    pub x: i64,
    pub y: i64,
    pub w: i64,
    pub h: i64,
    #[serde(default)]
    pub col_widths: Vec<i64>,
    #[serde(default)]
    pub row_heights: Vec<i64>,
    /// 纯文本单元格；合并单元格记 fidelity lossy
    #[serde(default)]
    pub cells: Vec<Vec<String>>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct UnsupportedElement {
    pub x: i64,
    pub y: i64,
    pub w: i64,
    pub h: i64,
    /// chart / smartart / media / freeform ...
    pub what: String,
    #[serde(default)]
    pub note: String,
}

// ---------------------------------------------------------------- 分发

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Element {
    Text(TextElement),
    Shape(ShapeElement),
    Image(ImageElement),
    Line(LineElement),
    Table(TableElement),
    Unsupported(UnsupportedElement),
}

impl<'de> Deserialize<'de> for Element {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let v = serde_json::Value::deserialize(d)?;
        from_value(v).map_err(serde::de::Error::custom)
    }
}

fn from_value(v: serde_json::Value) -> Result<Element, String> {
    let obj = v
        .as_object()
        .ok_or_else(|| "元素应为对象 {...}".to_string())?;
    let kind = obj
        .get("kind")
        .and_then(|k| k.as_str())
        .ok_or_else(|| "缺少 \"kind\" 字段".to_string())?;
    let mut rest = obj.clone();
    rest.remove("kind");
    let wrap = |e: serde_json::Error| prettify_schema_error(e.to_string());
    match kind {
        "text" => Ok(Element::Text(serde_json::from_value(serde_json::Value::Object(rest)).map_err(wrap)?)),
        "shape" => Ok(Element::Shape(serde_json::from_value(serde_json::Value::Object(rest)).map_err(wrap)?)),
        "image" => Ok(Element::Image(serde_json::from_value(serde_json::Value::Object(rest)).map_err(wrap)?)),
        "line" => Ok(Element::Line(serde_json::from_value(serde_json::Value::Object(rest)).map_err(wrap)?)),
        "table" => Ok(Element::Table(serde_json::from_value(serde_json::Value::Object(rest)).map_err(wrap)?)),
        "unsupported" => Ok(Element::Unsupported(serde_json::from_value(serde_json::Value::Object(rest)).map_err(wrap)?)),
        other => Err(format!(
            "未知元素类型 {other:?}，合法值：text / shape / image / line / table / unsupported"
        )),
    }
}

/// 把 serde 的 "unknown field `colour`" 翻译成带建议的人话
fn prettify_schema_error(msg: String) -> String {
    if let Some(start) = msg.find("unknown field `") {
        let rest = &msg[start + "unknown field `".len()..];
        if let Some(end) = rest.find('`') {
            let field = &rest[..end];
            let suggestions = suggest_for(field);
            if !suggestions.is_empty() {
                return format!("{msg}（你是不是想写 {}？）", suggestions.join(" 或 "));
            }
        }
    }
    msg
}

fn suggest_for(field: &str) -> Vec<String> {
    const ALL: [&str; 38] = [
        "x", "y", "w", "h", "paragraphs", "rotation", "word_wrap", "auto_size", "anchor",
        "shadow", "geometry", "fill_type", "fill_color", "line_color", "line_width_pt",
        "line_dash", "asset", "crop_pct", "color", "width_pt", "dash", "flip_h", "flip_v",
        "tail_end", "col_widths", "row_heights", "cells", "what", "note", "text", "bold",
        "italic", "underline", "size_pt", "font", "runs", "align", "level",
    ];
    let fl = field.to_lowercase();
    let mut out: Vec<String> = ALL
        .iter()
        .filter(|s| {
            let s = s.to_lowercase();
            s.contains(&fl) || fl.contains(&s)
        })
        .map(|s| s.to_string())
        .collect();
    if out.is_empty() {
        // 公共前缀 ≥ 3 也算近亲（colour → color）
        out = ALL
            .iter()
            .filter(|s| {
                let s = s.to_lowercase();
                let common = fl.chars().zip(s.chars()).take_while(|(a, b)| a == b).count();
                common >= 3
            })
            .map(|s| s.to_string())
            .collect();
    }
    out.truncate(3);
    out
}

// ---------------------------------------------------------------- 页与文档

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Slide {
    #[serde(default)]
    pub elements: Vec<Element>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub background_color: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Presentation {
    pub kind: String,
    pub width_emu: i64,
    pub height_emu: i64,
    pub slides: Vec<Slide>,
}

impl Default for Presentation {
    fn default() -> Self {
        Self { kind: "presentation".into(), width_emu: 12192000, height_emu: 6858000, slides: vec![] }
    }
}

impl Presentation {
    pub fn to_json_pretty(&self) -> String {
        serde_json::to_string_pretty(self).unwrap()
    }
}
