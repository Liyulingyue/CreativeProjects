use std::fs;
use std::io;
use std::path::Path;

pub const REG_HEADER: &str = "Windows Registry Editor Version 5.00";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum RegValueKind {
    Str,
    Dword,
    Qword,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegValue {
    pub name: Option<String>,
    pub kind: RegValueKind,
    pub data: String,
}

impl RegValue {
    pub fn string(name: Option<&str>, data: impl Into<String>) -> Self {
        Self {
            name: name.map(str::to_owned),
            kind: RegValueKind::Str,
            data: data.into(),
        }
    }

    #[allow(dead_code)]
    pub fn dword(name: Option<&str>, data: u32) -> Self {
        Self {
            name: name.map(str::to_owned),
            kind: RegValueKind::Dword,
            data: format!("{data:08x}"),
        }
    }

    #[allow(dead_code)]
    pub fn qword(name: Option<&str>, data: u64) -> Self {
        Self {
            name: name.map(str::to_owned),
            kind: RegValueKind::Qword,
            data: format!("{data:016x}"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegEntry {
    pub key_path: String,
    pub values: Vec<RegValue>,
}

impl RegEntry {
    pub fn new(key_path: impl Into<String>, values: Vec<RegValue>) -> Self {
        Self {
            key_path: key_path.into(),
            values,
        }
    }
}

pub fn escape_value(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            _ => out.push(ch),
        }
    }
    out
}

fn render_value(value: &RegValue) -> String {
    let name = match &value.name {
        Some(name) => format!("\"{}\"", escape_value(name)),
        None => "@".to_string(),
    };
    match value.kind {
        RegValueKind::Str => format!("{name}=\"{}\"", escape_value(&value.data)),
        RegValueKind::Dword => format!("{name}=dword:{}", value.data),
        RegValueKind::Qword => format!("{name}=qword:{}", value.data),
    }
}

pub fn render(entries: &[RegEntry]) -> String {
    let mut out = String::new();
    out.push_str(REG_HEADER);
    out.push_str("\r\n");
    for entry in entries {
        out.push_str("\r\n[");
        out.push_str(&entry.key_path);
        out.push_str("]\r\n");
        for value in &entry.values {
            out.push_str(&render_value(value));
            out.push_str("\r\n");
        }
    }
    out
}

pub fn encode_utf16le_with_bom(text: &str) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(text.len() * 2 + 2);
    bytes.extend_from_slice(&[0xFF, 0xFE]);
    for unit in text.encode_utf16() {
        bytes.extend_from_slice(&unit.to_le_bytes());
    }
    bytes
}

pub fn write_reg_file(path: &Path, entries: &[RegEntry]) -> io::Result<()> {
    let text = render(entries);
    fs::write(path, encode_utf16le_with_bom(&text))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escapes_backslash_and_quote() {
        assert_eq!(
            escape_value(r"C:\Program Files\a.exe"),
            r"C:\\Program Files\\a.exe"
        );
        assert_eq!(escape_value(r#"say "hi""#), r#"say \"hi\""#);
    }

    #[test]
    fn renders_default_and_named_values() {
        let entries = vec![RegEntry::new(
            r"HKEY_CURRENT_USER\Software\Classes\demo",
            vec![
                RegValue::string(None, "URL:demo Protocol"),
                RegValue::string(Some("URL Protocol"), ""),
            ],
        )];
        let text = render(&entries);
        assert!(text.starts_with("Windows Registry Editor Version 5.00\r\n"));
        assert!(text.contains("\r\n[HKEY_CURRENT_USER\\Software\\Classes\\demo]\r\n"));
        assert!(text.contains("@=\"URL:demo Protocol\"\r\n"));
        assert!(text.contains("\"URL Protocol\"=\"\"\r\n"));
    }

    #[test]
    fn encodes_utf16le_bom() {
        let bytes = encode_utf16le_with_bom("A");
        assert_eq!(bytes, vec![0xFF, 0xFE, b'A', 0x00]);
    }

    #[test]
    fn renders_dword_and_qword() {
        let entries = vec![RegEntry::new(
            "HKCU\\Test",
            vec![
                RegValue::dword(Some("Count"), 1),
                RegValue::qword(Some("Big"), 2),
            ],
        )];
        let text = render(&entries);
        assert!(text.contains("\"Count\"=dword:00000001"));
        assert!(text.contains("\"Big\"=qword:0000000000000002"));
    }
}
