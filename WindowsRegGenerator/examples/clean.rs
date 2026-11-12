use windows_reg_generator::registry;
use windows_reg_generator::scheme::validate_scheme;

fn main() -> Result<(), String> {
    let mut scheme = String::new();
    let mut prefix_match = false;
    let mut dry_run = false;

    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut index = 0;
    while index < args.len() {
        let flag = args[index].as_str();
        match flag {
            "--scheme" => {
                index += 1;
                scheme = args
                    .get(index)
                    .cloned()
                    .ok_or_else(|| "参数 --scheme 缺少取值".to_string())?;
            }
            "--prefix-match" => prefix_match = true,
            "--dry-run" => dry_run = true,
            "--help" | "-h" => {
                println!("用法: clean --scheme <前缀> [--prefix-match] [--dry-run]");
                return Ok(());
            }
            other => return Err(format!("未知参数：{other}")),
        }
        index += 1;
    }

    validate_scheme(scheme.trim())?;
    let found = registry::scan(scheme.trim(), prefix_match).map_err(|err| err.to_string())?;

    if found.is_empty() {
        println!("未找到与 {scheme} 相关的注册表键");
        return Ok(());
    }

    for item in &found {
        println!(
            "{}\n    URL Protocol: {}\n    命令: {}",
            item.full_path,
            item.has_url_protocol,
            item.command.as_deref().unwrap_or("-")
        );
    }

    if dry_run {
        println!("--dry-run 模式，未做任何删除");
        return Ok(());
    }

    let mut failed = 0usize;
    for item in &found {
        match registry::remove(item.hive, &item.key_name) {
            Ok(()) => println!("已删除 {}", item.full_path),
            Err(err) => {
                failed += 1;
                println!(
                    "删除失败 {}：{}",
                    item.full_path,
                    registry::error_message(&err)
                );
            }
        }
    }

    println!("完成：成功 {} 项，失败 {failed} 项", found.len() - failed);
    Ok(())
}
