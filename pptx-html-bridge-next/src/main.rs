//! phb —— pptx-html-bridge-next
//!
//! 架构：manifest（类型化 JSON）是唯一信源。
//!   PPTX → manifest（读取，theme 颜色折算 RGB，组合拍平，保真度报告）
//!   manifest → PPTX（写入，嵌入空白模板打底）
//!   manifest → HTML（纯视图，只给人看，永不被解析回读）
use clap::{Parser, Subcommand};
use std::path::{Path, PathBuf};

use phb::{error, model, ooxml, view};

use error::Result;

#[derive(Parser)]
#[command(name = "phb", about = "PPTX ⇄ 类型化 JSON manifest ⇄ HTML 视图（纯 Rust）")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// PPTX → manifest.json + assets/ + fidelity 报告
    Read {
        input: PathBuf,
        outdir: PathBuf,
    },
    /// manifest.json（+ 同目录 assets/）→ PPTX
    Write {
        manifest: PathBuf,
        output: PathBuf,
    },
    /// manifest + assets → HTML 预览
    View {
        manifest: PathBuf,
        outdir: PathBuf,
    },
    /// 读 → 写 → 再读，验证往返稳定性，并输出保真度报告
    Roundtrip {
        input: PathBuf,
        outdir: PathBuf,
    },
}

fn main() {
    let cli = Cli::parse();
    let res = match &cli.cmd {
        Cmd::Read { input, outdir } => cmd_read(input, outdir),
        Cmd::Write { manifest, output } => cmd_write(manifest, output),
        Cmd::View { manifest, outdir } => cmd_view(manifest, outdir),
        Cmd::Roundtrip { input, outdir } => cmd_roundtrip(input, outdir),
    };
    if let Err(e) = res {
        eprintln!("错误：{e}");
        std::process::exit(1);
    }
}

fn cmd_read(input: &PathBuf, outdir: &PathBuf) -> Result<()> {
    let out = ooxml::read::read_pptx(input)?;
    std::fs::create_dir_all(outdir.join("assets"))?;
    for (name, bytes) in &out.assets {
        std::fs::write(outdir.join("assets").join(name), bytes)?;
    }
    std::fs::write(outdir.join("manifest.json"), out.manifest.to_json_pretty())?;
    println!("页数：{}", out.manifest.slides.len());
    println!("元素：{}", out.manifest.slides.iter().map(|s| s.elements.len()).sum::<usize>());
    println!("--- 保真度报告 ---");
    for line in &out.fidelity {
        println!("· {line}");
    }
    println!("已写出 {}/manifest.json 与 assets/", outdir.display());
    Ok(())
}

fn cmd_write(manifest: &PathBuf, output: &PathBuf) -> Result<()> {
    let json = std::fs::read_to_string(manifest)?;
    let pres: model::Presentation = serde_json::from_str(&json)
        .map_err(|e| error::BridgeError::Manifest(format!("{}（第 {} 行第 {} 列附近）", e, e.line(), e.column())))?;
    let assets_dir = manifest.parent().unwrap_or(Path::new(".")).join("assets");
    ooxml::write::write_pptx(&pres, &assets_dir, output)?;
    println!("已写出 {}", output.display());
    Ok(())
}

fn cmd_view(manifest: &PathBuf, outdir: &PathBuf) -> Result<()> {
    let json = std::fs::read_to_string(manifest)?;
    let pres: model::Presentation = serde_json::from_str(&json)
        .map_err(|e| error::BridgeError::Manifest(e.to_string()))?;
    let assets_dir = manifest.parent().unwrap_or(Path::new(".")).join("assets");
    let mut assets: Vec<(String, Vec<u8>)> = Vec::new();
    let dir = std::fs::read_dir(&assets_dir)?;
    for entry in dir.flatten() {
        if entry.path().is_file() {
            assets.push((
                entry.file_name().to_string_lossy().to_string(),
                std::fs::read(entry.path())?,
            ));
        }
    }
    view::view_html(&pres, &assets, outdir)?;
    println!("已写出 {}/index.html", outdir.display());
    Ok(())
}

fn cmd_roundtrip(input: &PathBuf, outdir: &PathBuf) -> Result<()> {
    std::fs::create_dir_all(outdir)?;
    let out1 = ooxml::read::read_pptx(input)?;
    std::fs::create_dir_all(outdir.join("assets"))?;
    for (name, bytes) in &out1.assets {
        std::fs::write(outdir.join("assets").join(name), bytes)?;
    }
    let tmp_pptx = outdir.join("roundtrip.pptx");
    ooxml::write::write_pptx(&out1.manifest, &outdir.join("assets"), &tmp_pptx)?;

    let out2 = ooxml::read::read_pptx(&tmp_pptx)?;
    let v1 = serde_json::to_string_pretty(&out1.manifest)?;
    let v2 = serde_json::to_string_pretty(&out2.manifest)?;
    let stable = v1 == v2;

    let rebuilt = outdir.join("rebuilt.pptx");
    std::fs::write(&rebuilt, std::fs::read(&tmp_pptx)?)?;
    std::fs::write(outdir.join("manifest.json"), out1.manifest.to_json_pretty())?;
    std::fs::write(outdir.join("manifest2.json"), out2.manifest.to_json_pretty())?;

    println!("页数：{}", out1.manifest.slides.len());
    println!("--- 保真度报告 ---");
    for line in &out1.fidelity {
        println!("· {line}");
    }
    println!("--- 往返稳定性 ---");
    if stable {
        println!("STABLE：第二轮转换与第一轮的 manifest 完全一致");
    } else {
        let diffs = diff_slides(&out1.manifest, &out2.manifest);
        println!("UNSTABLE：{diffs} 处差异");
        println!("（多为 unsupported 占位在第二轮固化为形状——属预期）");
    }
    println!("已写出 {}/manifest.json、{}/rebuilt.pptx", outdir.display(), outdir.display());
    Ok(())
}

fn diff_slides(a: &model::Presentation, b: &model::Presentation) -> usize {
    let va = serde_json::to_value(a).unwrap();
    let vb = serde_json::to_value(b).unwrap();
    count_diff(&va, &vb)
}

fn count_diff(a: &serde_json::Value, b: &serde_json::Value) -> usize {
    match (a, b) {
        (serde_json::Value::Array(x), serde_json::Value::Array(y)) => {
            let n = x.len().max(y.len());
            (0..n)
                .map(|i| {
                    count_diff(
                        x.get(i).unwrap_or(&serde_json::Value::Null),
                        y.get(i).unwrap_or(&serde_json::Value::Null),
                    )
                })
                .sum()
        }
        (serde_json::Value::Object(x), serde_json::Value::Object(y)) => {
            let keys: std::collections::HashSet<_> = x.keys().chain(y.keys()).collect();
            keys.iter()
                .map(|k| {
                    count_diff(
                        x.get(k.as_str()).unwrap_or(&serde_json::Value::Null),
                        y.get(k.as_str()).unwrap_or(&serde_json::Value::Null),
                    )
                })
                .sum()
        }
        (x, y) if x != y => 1,
        _ => 0,
    }
}
