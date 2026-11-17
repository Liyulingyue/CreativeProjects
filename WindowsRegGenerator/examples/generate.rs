use std::path::PathBuf;

use windows_reg_generator::regfile;
use windows_reg_generator::scheme::{validate_exe_path, validate_scheme, ProtocolConfig, Scope};

fn main() -> Result<(), String> {
    let mut scheme = "myapp".to_string();
    let mut exe = String::new();
    let mut out: Option<PathBuf> = None;
    let mut scope = Scope::CurrentUser;
    let mut include_icon = true;
    let mut friendly_name: Option<String> = None;

    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut index = 0;
    while index < args.len() {
        let flag = args[index].as_str();
        let take = |index: &mut usize| -> Result<String, String> {
            *index += 1;
            args.get(*index)
                .cloned()
                .ok_or_else(|| format!("参数 {flag} 缺少取值"))
        };
        match flag {
            "--scheme" => scheme = take(&mut index)?,
            "--exe" => exe = take(&mut index)?,
            "--out" => out = Some(PathBuf::from(take(&mut index)?)),
            "--scope" => {
                let value = take(&mut index)?;
                scope = match value.as_str() {
                    "current-user" => Scope::CurrentUser,
                    "all-users" => Scope::AllUsers,
                    other => return Err(format!("未知的作用域：{other}")),
                };
            }
            "--friendly-name" => friendly_name = Some(take(&mut index)?),
            "--no-icon" => include_icon = false,
            "--help" | "-h" => {
                println!("用法: generate --scheme <前缀> --exe <EXE路径> [--out <输出.reg>] [--scope current-user|all-users] [--friendly-name <名称>] [--no-icon]");
                return Ok(());
            }
            other => return Err(format!("未知参数：{other}")),
        }
        index += 1;
    }

    validate_scheme(scheme.trim())?;
    let exe_path = PathBuf::from(exe.trim());
    validate_exe_path(&exe_path)?;

    let out = out.unwrap_or_else(|| PathBuf::from(format!("{}.reg", scheme.trim())));

    let config = ProtocolConfig {
        scheme: scheme.trim().to_string(),
        exe_path,
        scope,
        friendly_name,
        include_icon,
    };

    regfile::write_reg_file(&out, &config.to_entries()).map_err(|err| err.to_string())?;
    println!("已生成 {}", out.display());
    Ok(())
}
