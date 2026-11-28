//! 严格 zip 打开失败时的兜底：手工解析中心目录，按"最后一条 wins"去重重建临时包。
//! 真实世界的 PPTX 偶有重复 part 名（PowerPoint / python zipfile 都容忍），不能因此拒读。
use crate::error::{BridgeError, Result};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

pub struct LenientZip {
    pub archive: zip::ZipArchive<fs::File>,
    /// 修复时产生的临时文件（读完后删除）
    pub temp_path: Option<PathBuf>,
}

pub fn open_lenient(path: &Path) -> Result<LenientZip> {
    let file = fs::File::open(path)?;
    match zip::ZipArchive::new(file) {
        Ok(a) => Ok(LenientZip { archive: a, temp_path: None }),
        Err(e) => {
            let msg = e.to_string();
            if !msg.contains("Duplicate") {
                return Err(e.into());
            }
            let cleaned = repair_duplicates(path)?;
            let archive = zip::ZipArchive::new(fs::File::open(&cleaned)?)?;
            Ok(LenientZip { archive, temp_path: Some(cleaned) })
        }
    }
}

/// 手工解析中心目录 → 取每个名字的最后一条 → 重建干净的临时 zip
fn repair_duplicates(path: &Path) -> Result<PathBuf> {
    let bytes = fs::read(path)?;
    let eocd = find_eocd(&bytes).ok_or_else(|| BridgeError::Other("找不到 ZIP EOCD".into()))?;
    let cd_size = u32::from_le_bytes(bytes[eocd + 12..eocd + 16].try_into().unwrap()) as usize;
    let cd_off = u32::from_le_bytes(bytes[eocd + 16..eocd + 20].try_into().unwrap()) as usize;
    if cd_off == 0xFFFF_FFFF {
        return Err(BridgeError::Other("ZIP64 包暂不支持修复重复 part".into()));
    }

    let mut entries: std::collections::HashMap<String, (u16, Vec<u8>)> = std::collections::HashMap::new();
    let mut order: Vec<String> = Vec::new();
    let mut p = cd_off;
    let cd_end = cd_off + cd_size;
    while p + 46 <= cd_end && &bytes[p..p + 4] == b"PK\x01\x02" {
        let method = u16::from_le_bytes(bytes[p + 10..p + 12].try_into().unwrap());
        let csize = u32::from_le_bytes(bytes[p + 20..p + 24].try_into().unwrap()) as usize;
        let nlen = u16::from_le_bytes(bytes[p + 28..p + 30].try_into().unwrap()) as usize;
        let elen = u16::from_le_bytes(bytes[p + 30..p + 32].try_into().unwrap()) as usize;
        let clen = u16::from_le_bytes(bytes[p + 32..p + 34].try_into().unwrap()) as usize;
        let lho = u32::from_le_bytes(bytes[p + 42..p + 46].try_into().unwrap()) as usize;
        let name = String::from_utf8_lossy(&bytes[p + 46..p + 46 + nlen]).to_string();
        // 局部头：取数据起点（局部头的名字/扩展字段长度可能与中心目录不同）
        if lho + 30 > bytes.len() || &bytes[lho..lho + 4] != b"PK\x03\x04" {
            return Err(BridgeError::Other(format!("局部头损坏：{name}")));
        }
        let lnlen = u16::from_le_bytes(bytes[lho + 26..lho + 28].try_into().unwrap()) as usize;
        let lelen = u16::from_le_bytes(bytes[lho + 28..lho + 30].try_into().unwrap()) as usize;
        let dstart = lho + 30 + lnlen + lelen;
        let dend = dstart.checked_add(csize).ok_or_else(|| BridgeError::Other(format!("数据越界：{name}")))?;
        if dend > bytes.len() {
            return Err(BridgeError::Other(format!("数据越界：{name}")));
        }
        let data = bytes[dstart..dend].to_vec();
        if !entries.contains_key(&name) {
            order.push(name.clone());
        }
        entries.insert(name, (method, data)); // 最后一条 wins
        p += 46 + nlen + elen + clen;
    }

    // 重建：deflate 重新压缩（内容不变，仅打包形态）
    let temp = std::env::temp_dir().join(format!(
        "phb_repair_{}_{}.pptx",
        std::process::id(),
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.subsec_nanos()).unwrap_or(0)
    ));
    let file = fs::File::create(&temp)?;
    let mut zw = zip::ZipWriter::new(file);
    let opts = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    for name in &order {
        let (method, data) = &entries[name];
        let o = opts.compression_method(if *method == 0 {
            zip::CompressionMethod::Stored
        } else {
            zip::CompressionMethod::Deflated
        });
        zw.start_file(name.clone(), o)
            .map_err(|e| BridgeError::Other(format!("重建失败：{e}")))?;
        zw.write_all(data)?;
    }
    zw.finish()?;
    Ok(temp)
}

/// 从尾部找 EOCD 签名（PK\x05\x06）
fn find_eocd(b: &[u8]) -> Option<usize> {
    if b.len() < 22 {
        return None;
    }
    let start = b.len().saturating_sub(22 + 65535);
    (start..b.len() - 21).rev().find(|&i| &b[i..i + 4] == b"PK\x05\x06")
}
