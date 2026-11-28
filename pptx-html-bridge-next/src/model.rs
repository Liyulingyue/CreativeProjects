use serde::{Deserialize, Serialize};
use thiserror::Error;

/// manifest 的类型化模型 —— 唯一信源。
/// 所有几何量一律 EMU（i64），颜色一律 RRGGBB，角度一律度（f64）。
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

// ---------------------------------------------------------------- 链接 / 项目符号 / 渐变

/// 超链接：外部 URL 或内部跳页（slide 为 1 起页号）。
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(deny_unknown_fields)]
pub struct Link {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// 跳到第几页（1 起）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub slide: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tooltip: Option<String>,
}

impl Link {
    pub fn url(s: impl Into<String>) -> Self {
        Self { url: Some(s.into()), ..Default::default() }
    }
    pub fn slide(n: u32) -> Self {
        Self { slide: Some(n), ..Default::default() }
    }
}

/// 项目符号：无 / 字符 / 自动编号。
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Bullet {
    /// none | char | auto_num
    pub kind: String,
    /// kind=char 时的符号
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub character: Option<String>,
    /// 符号字体（buFont）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub font: Option<String>,
    /// RRGGBB（buClr）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    /// 相对文字大小的百分比（buSzPct，100 = 同大）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size_pct: Option<f64>,
    /// kind=auto_num 时的编号样式：arabicPeriod / alphaLcPeriod / ...
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub num_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub num_start: Option<i64>,
}

impl Default for Bullet {
    fn default() -> Self {
        Self {
            kind: "char".into(),
            character: None,
            font: None,
            color: None,
            size_pct: None,
            num_type: None,
            num_start: None,
        }
    }
}

impl Bullet {
    /// 无项目符号
    pub fn none() -> Self {
        Self { kind: "none".into(), ..Default::default() }
    }
}

/// 渐变停靠点
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct GradientStop {
    /// 0-100
    pub pos_pct: f64,
    /// RRGGBB
    pub color: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alpha_pct: Option<f64>,
}

// ---------------------------------------------------------------- 文字

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
    /// latin 字体的 pitchFamily（影响字体族选择）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub font_pitch_family: Option<i64>,
    /// latin 字符集（charset，0 = ANSI）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub font_charset: Option<i64>,
    /// east-asian 字符集（-122 = GB2312；影响 CJK 字形选择）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub font_charset_ea: Option<i64>,
    /// latin/ea 字体的 PANOSE 指纹（字体替换时选字形用）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub font_panose: Option<String>,
    /// sym（符号字体）typeface
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub font_sym: Option<String>,
    /// cs（complex script）typeface；None = 不写 cs
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub font_cs: Option<String>,
    /// 超链接
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub link: Option<Link>,
    /// kerning 阈值（rPr@kern，1/100 pt）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kern: Option<i64>,
    /// 字符间距（rPr@spc，1/100 pt）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spc: Option<i64>,
    /// 语言标签；None = 写出 zh-CN（读侧 zh-CN 归一为 None，保证往返）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lang: Option<String>,
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
    /// 项目符号
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bullet: Option<Bullet>,
    /// 左缩进 EMU（pPr@marL）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mar_l: Option<i64>,
    /// 首行缩进 EMU（pPr@indent，负值 = 悬挂）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub indent: Option<i64>,
    /// 段前距 pt（spcBef）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub space_before_pt: Option<f64>,
    /// 段后距 pt（spcAft）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub space_after_pt: Option<f64>,
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
    /// 阴影对齐（outerShdw@algn：bl / ctr / tr ...）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub algn: Option<String>,
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
    /// 文本框内边距 EMU [l, t, r, b]（bodyPr@lIns 等）；None = 应用默认
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub insets: Option<[i64; 4]>,
    /// normAutofit 的 fontScale（百分比，92.5 = 92500）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub font_scale_pct: Option<f64>,
    /// normAutofit 的 lnSpcReduction（百分比）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ln_spc_reduction_pct: Option<f64>,
}

impl Default for TextElement {
    fn default() -> Self {
        Self {
            x: 0,
            y: 0,
            w: 0,
            h: 0,
            paragraphs: vec![],
            rotation: None,
            word_wrap: None,
            auto_size: None,
            anchor: None,
            shadow: None,
            insets: None,
            font_scale_pct: None,
            ln_spc_reduction_pct: None,
        }
    }
}

// ---------------------------------------------------------------- 形状

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
    /// solid | none | gradient
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fill_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fill_color: Option<String>,
    /// 纯色填充不透明度 0-100
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fill_alpha_pct: Option<f64>,
    /// fill_type=gradient 时的停靠点
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fill_stops: Option<Vec<GradientStop>>,
    /// 渐变方向角（度，0 = 从左向右）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fill_angle_deg: Option<f64>,
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
    /// 形状文本区内边距 EMU [l, t, r, b]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub insets: Option<[i64; 4]>,
    /// 文字纵向对齐：top | bottom（None = 写出 ctr，读侧 ctr 归一为 None）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub anchor: Option<String>,
    /// prstGeom 的 adj 调整值（如 roundRect 圆角，val 2874 → Some(2874)）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub adj_val: Option<i64>,
    /// 占位符类型（ph@type：title / body / ctrTitle ...）；None = 普通形状
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ph_type: Option<String>,
    /// 占位符 idx（ph@idx）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ph_idx: Option<u32>,
    /// 无自身 xfrm：几何完全继承版式（写出时 spPr 留空）
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub inherit_geom: bool,
}

fn default_geometry() -> String {
    "rect".to_string()
}

impl Default for ShapeElement {
    fn default() -> Self {
        Self {
            x: 0,
            y: 0,
            w: 0,
            h: 0,
            geometry: default_geometry(),
            fill_type: None,
            fill_color: None,
            fill_alpha_pct: None,
            fill_stops: None,
            fill_angle_deg: None,
            line_color: None,
            line_width_pt: None,
            line_dash: None,
            rotation: None,
            shadow: None,
            paragraphs: None,
            insets: None,
            anchor: None,
            adj_val: None,
            ph_type: None,
            ph_idx: None,
            inherit_geom: false,
        }
    }
}

// ---------------------------------------------------------------- 自由形状

/// 自由形状：custGeom 路径。坐标一律在子路径自己的 w × h 坐标系里，
/// 写入时按 path@w/h 原样还原，外框缩放到 x/y/w/h。
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct FreeformElement {
    pub x: i64,
    pub y: i64,
    pub w: i64,
    pub h: i64,
    /// 路径坐标系宽（custGeom path@w），None = 用 w
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path_w: Option<i64>,
    /// 路径坐标系高（custGeom path@h），None = 用 h
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path_h: Option<i64>,
    pub subpaths: Vec<SubPath>,
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
    pub rotation: Option<f64>,
}

impl Default for FreeformElement {
    fn default() -> Self {
        Self {
            x: 0,
            y: 0,
            w: 0,
            h: 0,
            path_w: None,
            path_h: None,
            subpaths: vec![],
            fill_type: None,
            fill_color: None,
            line_color: None,
            line_width_pt: None,
            rotation: None,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(deny_unknown_fields)]
pub struct SubPath {
    /// 本子路径自己的坐标空间宽高（多 path 的 custGeom 各自独立）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub w: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub h: Option<i64>,
    #[serde(default)]
    pub closed: bool,
    pub cmds: Vec<PathCmd>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "c", rename_all = "snake_case")]
pub enum PathCmd {
    Move { x: i64, y: i64 },
    Line { x: i64, y: i64 },
    Cubic { x1: i64, y1: i64, x2: i64, y2: i64, x: i64, y: i64 },
    Quad { x1: i64, y1: i64, x: i64, y: i64 },
    /// 椭圆弧：w_r/h_r 为半径（路径坐标单位），角度为度
    Arc { w_r: i64, h_r: i64, st_ang_deg: f64, sw_ang_deg: f64 },
}

// ---------------------------------------------------------------- 图片 / 线 / 表

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
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
    /// 点击图片跳转
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub link: Option<Link>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
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
    /// 起端样式：triangle / arrow / none ...
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub head_end: Option<String>,
    /// 箭头等尾端样式：triangle / arrow / none ...
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tail_end: Option<String>,
}

/// 表格单元格：兼容旧版纯字符串写法（`"甲"` 等价 `{"text": "甲"}`）。
#[derive(Serialize, Clone, Debug, PartialEq, Default)]
#[serde(deny_unknown_fields)]
pub struct TableCell {
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fill_color: Option<String>,
    /// 跨列数（gridSpan）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub col_span: Option<u32>,
    /// 跨行数（rowSpan）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub row_span: Option<u32>,
    /// 被合并吃掉的延续格：写出时跳过
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub merged_away: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bold: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size_pt: Option<f64>,
    /// left | center | right
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub align: Option<String>,
}

impl From<&str> for TableCell {
    fn from(s: &str) -> Self {
        Self { text: s.into(), ..Default::default() }
    }
}

impl From<String> for TableCell {
    fn from(s: String) -> Self {
        Self { text: s, ..Default::default() }
    }
}

impl<'de> Deserialize<'de> for TableCell {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let v = serde_json::Value::deserialize(d)?;
        match v {
            serde_json::Value::String(s) => Ok(Self { text: s, ..Default::default() }),
            serde_json::Value::Object(_) => {
                // 走结构体的严格反序列化（保留 deny_unknown_fields 报错）
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct Raw {
                    text: String,
                    fill_color: Option<String>,
                    col_span: Option<u32>,
                    row_span: Option<u32>,
                    #[serde(default)]
                    merged_away: bool,
                    bold: Option<bool>,
                    color: Option<String>,
                    size_pt: Option<f64>,
                    align: Option<String>,
                }
                let r: Raw = serde_json::from_value(v).map_err(serde::de::Error::custom)?;
                Ok(Self {
                    text: r.text,
                    fill_color: r.fill_color,
                    col_span: r.col_span,
                    row_span: r.row_span,
                    merged_away: r.merged_away,
                    bold: r.bold,
                    color: r.color,
                    size_pt: r.size_pt,
                    align: r.align,
                })
            }
            _ => Err(serde::de::Error::custom("单元格应为字符串或对象")),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
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
    /// 单元格网格；合并单元格：origin 记 col_span/row_span，延续格 merged_away=true
    #[serde(default)]
    pub cells: Vec<Vec<TableCell>>,
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
    Freeform(FreeformElement),
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
        "freeform" => Ok(Element::Freeform(serde_json::from_value(serde_json::Value::Object(rest)).map_err(wrap)?)),
        "unsupported" => Ok(Element::Unsupported(serde_json::from_value(serde_json::Value::Object(rest)).map_err(wrap)?)),
        other => Err(format!(
            "未知元素类型 {other:?}，合法值：text / shape / image / line / table / freeform / unsupported"
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
    const ALL: [&str; 74] = [
        "x", "y", "w", "h", "paragraphs", "rotation", "word_wrap", "auto_size", "anchor",
        "shadow", "geometry", "fill_type", "fill_color", "fill_alpha_pct", "fill_stops",
        "fill_angle_deg", "line_color", "line_width_pt", "line_dash", "asset", "crop_pct",
        "color", "width_pt", "dash", "flip_h", "flip_v", "head_end", "tail_end",
        "col_widths", "row_heights", "cells", "text", "col_span", "row_span",
        "merged_away", "bold", "size_pt", "align", "what", "note", "path_w", "path_h",
        "subpaths", "closed", "cmds", "runs", "level", "bullet", "mar_l", "indent",
        "space_before_pt", "space_after_pt", "link", "url", "slide", "tooltip",
        "character", "num_type", "num_start", "italic", "underline",
        "background_color", "background_image", "notes", "kind", "width_emu",
        "height_emu", "slides", "theme", "pos_pct", "alpha_pct", "name",
        "major_font", "minor_font",
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

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(deny_unknown_fields)]
pub struct Slide {
    #[serde(default)]
    pub elements: Vec<Element>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub background_color: Option<String>,
    /// 背景图片（assets 文件名）；与 background_color 同时存在时图片优先
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub background_image: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    /// 本页版式编号（template pack 里的 slideLayoutN，None = 1）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layout: Option<u32>,
}

/// 从源文件提取的主题（写出时回填 theme1.xml，让二次编辑配色/字体继承原稿）。
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(deny_unknown_fields)]
pub struct ThemeManifest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default)]
    pub colors: ThemeColors,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub major_font: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub minor_font: Option<String>,
}

/// 源文件的母版资源包：写出时整体替换内嵌空白模板的母版/版式/主题/默认文本样式。
/// None（或与内嵌空白模板等价）= 使用默认母版。
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(deny_unknown_fields)]
pub struct TemplatePack {
    pub theme_xml: String,
    pub master_xml: String,
    pub master_rels_xml: String,
    #[serde(default)]
    pub layouts: Vec<LayoutPart>,
    /// 原 presentation.xml 的 <p:defaultTextStyle> 整块
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_text_style: Option<String>,
    /// 母版/版式引用的媒体（asset 名对应 assets 目录文件）
    #[serde(default)]
    pub media: Vec<MediaRef>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct LayoutPart {
    /// 版式编号（1 起，写入时沿用原编号）
    pub number: u32,
    pub xml: String,
    pub rels_xml: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct MediaRef {
    /// 包内路径（如 ppt/media/image5.png）
    pub path: String,
    /// assets 目录里的文件名
    pub asset: String,
}

/// clrScheme 的 12 个槽位（固定顺序：dk1 lt1 dk2 lt2 accent1-6 hlink folHlink）
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ThemeColors {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dk1: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lt1: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dk2: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lt2: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub accent1: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub accent2: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub accent3: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub accent4: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub accent5: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub accent6: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hlink: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fol_hlink: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Presentation {
    pub kind: String,
    pub width_emu: i64,
    pub height_emu: i64,
    pub slides: Vec<Slide>,
    /// 源文件主题；写出时回填 theme1.xml（有 template 时被其 raw 主题覆盖）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub theme: Option<ThemeManifest>,
    /// 源文件母版资源包；None = 内嵌空白母版
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub template: Option<TemplatePack>,
}

impl Default for Presentation {
    fn default() -> Self {
        Self {
            kind: "presentation".into(),
            width_emu: 12192000,
            height_emu: 6858000,
            slides: vec![],
            theme: None,
            template: None,
        }
    }
}

impl Presentation {
    pub fn to_json_pretty(&self) -> String {
        serde_json::to_string_pretty(self).unwrap()
    }
}
