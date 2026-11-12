use std::path::PathBuf;

use crate::regfile::{RegEntry, RegValue};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Scope {
    #[default]
    CurrentUser,
    AllUsers,
}

impl Scope {
    pub fn classes_root(self) -> &'static str {
        match self {
            Scope::CurrentUser => r"HKEY_CURRENT_USER\Software\Classes",
            Scope::AllUsers => r"HKEY_LOCAL_MACHINE\SOFTWARE\Classes",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProtocolConfig {
    pub scheme: String,
    pub exe_path: PathBuf,
    pub scope: Scope,
    pub friendly_name: Option<String>,
    pub include_icon: bool,
}

impl ProtocolConfig {
    pub fn scheme_key(&self) -> String {
        format!("{}\\{}", self.scope.classes_root(), self.scheme)
    }

    pub fn command_key(&self) -> String {
        format!(
            "{}\\{}\\shell\\open\\command",
            self.scope.classes_root(),
            self.scheme
        )
    }

    pub fn command_line(&self) -> String {
        format!("\"{}\" \"%1\"", self.exe_path.display())
    }

    pub fn icon_value(&self) -> String {
        format!("{},0", self.exe_path.display())
    }

    pub fn to_entries(&self) -> Vec<RegEntry> {
        let mut values = vec![
            RegValue::string(None, format!("URL:{} Protocol", self.scheme)),
            RegValue::string(Some("URL Protocol"), ""),
        ];
        if let Some(name) = &self.friendly_name {
            values.push(RegValue::string(Some("FriendlyTypeName"), name));
        }
        if self.include_icon {
            values.push(RegValue::string(Some("DefaultIcon"), self.icon_value()));
        }

        vec![
            RegEntry::new(self.scheme_key(), values),
            RegEntry::new(
                self.command_key(),
                vec![RegValue::string(None, self.command_line())],
            ),
        ]
    }
}

pub fn validate_scheme(scheme: &str) -> Result<(), String> {
    if scheme.is_empty() {
        return Err("协议前缀不能为空".to_string());
    }
    if scheme.len() > 64 {
        return Err("协议前缀过长（最多 64 个字符）".to_string());
    }
    let first = scheme.chars().next().unwrap();
    if !first.is_ascii_alphabetic() {
        return Err("协议前缀必须以字母开头".to_string());
    }
    for ch in scheme.chars() {
        if !(ch.is_ascii_alphanumeric() || ch == '+' || ch == '-' || ch == '.') {
            return Err(format!("协议前缀包含非法字符：{ch}"));
        }
    }
    if scheme.ends_with('.') {
        return Err("协议前缀不能以 '.' 结尾".to_string());
    }
    Ok(())
}

pub fn validate_exe_path(path: &std::path::Path) -> Result<(), String> {
    if path.as_os_str().is_empty() {
        return Err("请选择 EXE 文件".to_string());
    }
    if !path.is_file() {
        return Err(format!("文件不存在：{}", path.display()));
    }
    let extension = path
        .extension()
        .map(|ext| ext.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    if extension != "exe" {
        return Err("目标文件不是 .exe".to_string());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(scheme: &str, exe: &str, scope: Scope) -> ProtocolConfig {
        ProtocolConfig {
            scheme: scheme.to_string(),
            exe_path: PathBuf::from(exe),
            scope,
            friendly_name: None,
            include_icon: false,
        }
    }

    #[test]
    fn builds_hkcu_key_paths() {
        let cfg = config("myapp", r"C:\Tools\myapp.exe", Scope::CurrentUser);
        assert_eq!(
            cfg.scheme_key(),
            r"HKEY_CURRENT_USER\Software\Classes\myapp"
        );
        assert_eq!(
            cfg.command_key(),
            r"HKEY_CURRENT_USER\Software\Classes\myapp\shell\open\command"
        );
        assert_eq!(cfg.command_line(), r#""C:\Tools\myapp.exe" "%1""#);
    }

    #[test]
    fn builds_hklm_key_paths() {
        let cfg = config("myapp", r"C:\Tools\myapp.exe", Scope::AllUsers);
        assert_eq!(
            cfg.scheme_key(),
            r"HKEY_LOCAL_MACHINE\SOFTWARE\Classes\myapp"
        );
    }

    #[test]
    fn entries_contain_url_protocol_marker() {
        let cfg = config("myapp", r"C:\Tools\myapp.exe", Scope::CurrentUser);
        let entries = cfg.to_entries();
        assert_eq!(entries.len(), 2);
        assert!(entries[0]
            .values
            .iter()
            .any(|v| v.name.as_deref() == Some("URL Protocol")));
        assert_eq!(entries[1].values[0].data, r#""C:\Tools\myapp.exe" "%1""#);
    }

    #[test]
    fn validates_scheme() {
        assert!(validate_scheme("myapp").is_ok());
        assert!(validate_scheme("my-app").is_ok());
        assert!(validate_scheme("my.app").is_ok());
        assert!(validate_scheme("").is_err());
        assert!(validate_scheme("1app").is_err());
        assert!(validate_scheme("my app").is_err());
        assert!(validate_scheme("my/app").is_err());
        assert!(validate_scheme("myapp.").is_err());
    }
}
