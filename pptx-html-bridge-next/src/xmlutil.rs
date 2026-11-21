//! roxmltree / XML 字符串的小工具。
use roxmltree::Node;

pub const NS_A: &str = "http://schemas.openxmlformats.org/drawingml/2006/main";
pub const NS_P: &str = "http://schemas.openxmlformats.org/presentationml/2006/main";
pub const NS_R: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";

/// 无命名空间属性（rot / flipH / val 这类）
pub fn attr<'a, 'b, 'c>(n: roxmltree::Node<'a, 'b>, local: &'c str) -> Option<&'a str> {
    n.attributes().into_iter().find(|a| a.namespace().is_none() && a.name() == local).map(|a| a.value())
}

/// 带命名空间属性（r:embed 这类）
pub fn attr_ns<'a, 'b, 'c>(n: roxmltree::Node<'a, 'b>, ns: &'c str, local: &'c str) -> Option<&'a str> {
    n.attributes().into_iter().find(|a| a.namespace() == Some(ns) && a.name() == local).map(|a| a.value())
}

pub fn child<'a, 'b>(n: Node<'a, 'b>, local: &str) -> Option<Node<'a, 'b>> {
    n.children().find(|c| c.tag_name().name() == local)
}

pub fn children<'a, 'b>(n: Node<'a, 'b>, local: &str) -> Vec<Node<'a, 'b>> {
    n.children().filter(|c| c.tag_name().name() == local).collect()
}

/// 后代里按 local name 找第一个
pub fn find<'a, 'b>(n: Node<'a, 'b>, local: &str) -> Option<Node<'a, 'b>> {
    n.descendants().find(|c| c.tag_name().name() == local)
}

pub fn text_of(n: Node) -> String {
    let mut s = String::new();
    for t in n.descendants() {
        if t.tag_name().name() == "t" {
            s.push_str(t.text().unwrap_or(""));
        }
    }
    s
}

pub fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

pub fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
