use std::io;

use winreg::enums::{
    HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_ENUMERATE_SUB_KEYS, KEY_READ, KEY_SET_VALUE,
    KEY_WOW64_64KEY, KEY_WRITE,
};
use winreg::{RegKey, HKEY};

const CLASSES_REL_CURRENT: &str = r"Software\Classes";
const CLASSES_REL_ALL: &str = r"SOFTWARE\Classes";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hive {
    CurrentUser,
    AllUsers,
}

impl Hive {
    pub fn label(self) -> &'static str {
        match self {
            Hive::CurrentUser => "HKEY_CURRENT_USER",
            Hive::AllUsers => "HKEY_LOCAL_MACHINE",
        }
    }

    pub fn handle(self) -> HKEY {
        match self {
            Hive::CurrentUser => HKEY_CURRENT_USER,
            Hive::AllUsers => HKEY_LOCAL_MACHINE,
        }
    }

    pub fn classes_rel(self) -> &'static str {
        match self {
            Hive::CurrentUser => CLASSES_REL_CURRENT,
            Hive::AllUsers => CLASSES_REL_ALL,
        }
    }
}

#[derive(Debug, Clone)]
pub struct FoundKey {
    pub hive: Hive,
    pub key_name: String,
    pub full_path: String,
    pub has_url_protocol: bool,
    pub command: Option<String>,
    pub selected: bool,
}

fn read_flag(hive: Hive, key_name: &str, value_name: &str) -> bool {
    let root = RegKey::predef(hive.handle());
    root.open_subkey_with_flags(
        format!("{}\\{key_name}", hive.classes_rel()),
        KEY_READ | KEY_WOW64_64KEY,
    )
    .and_then(|key| key.get_value::<String, _>(value_name))
    .is_ok()
}

fn read_command(hive: Hive, key_name: &str) -> Option<String> {
    let root = RegKey::predef(hive.handle());
    root.open_subkey_with_flags(
        format!("{}\\{key_name}\\shell\\open\\command", hive.classes_rel()),
        KEY_READ | KEY_WOW64_64KEY,
    )
    .ok()
    .and_then(|key| key.get_value::<String, _>("").ok())
}

pub fn scan(scheme: &str, prefix_match: bool) -> io::Result<Vec<FoundKey>> {
    let needle = scheme.trim().to_ascii_lowercase();
    let mut found = Vec::new();

    for hive in [Hive::CurrentUser, Hive::AllUsers] {
        let root = RegKey::predef(hive.handle());
        let Ok(classes) = root.open_subkey_with_flags(
            hive.classes_rel(),
            KEY_READ | KEY_ENUMERATE_SUB_KEYS | KEY_WOW64_64KEY,
        ) else {
            continue;
        };

        for key_name in classes.enum_keys().flatten() {
            let matched = if prefix_match {
                key_name.to_ascii_lowercase().starts_with(&needle)
            } else {
                key_name.eq_ignore_ascii_case(scheme.trim())
            };
            if !matched {
                continue;
            }

            found.push(FoundKey {
                hive,
                full_path: format!("{}\\{}\\{key_name}", hive.label(), hive.classes_rel()),
                has_url_protocol: read_flag(hive, &key_name, "URL Protocol"),
                command: read_command(hive, &key_name),
                key_name,
                selected: true,
            });
        }
    }

    Ok(found)
}

pub fn remove(hive: Hive, key_name: &str) -> io::Result<()> {
    let root = RegKey::predef(hive.handle());
    let classes = root.open_subkey_with_flags(
        hive.classes_rel(),
        KEY_READ | KEY_WRITE | KEY_SET_VALUE | KEY_ENUMERATE_SUB_KEYS | KEY_WOW64_64KEY,
    )?;
    classes.delete_subkey_all(key_name)
}

pub fn is_elevated() -> bool {
    let root = RegKey::predef(HKEY_LOCAL_MACHINE);
    root.open_subkey_with_flags(
        CLASSES_REL_ALL,
        KEY_READ | KEY_WRITE | KEY_SET_VALUE | KEY_WOW64_64KEY,
    )
    .is_ok()
}

pub fn error_message(error: &io::Error) -> String {
    match error.raw_os_error() {
        Some(5) => format!("{error}（访问被拒绝，需要管理员权限）"),
        Some(2) => format!("{error}（键不存在）"),
        _ => error.to_string(),
    }
}
