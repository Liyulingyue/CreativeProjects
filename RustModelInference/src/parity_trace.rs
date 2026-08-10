use serde_json::json;
use std::fs::OpenOptions;
use std::io::{self, Write};
use std::path::PathBuf;

fn trace_path() -> io::Result<PathBuf> {
    std::env::var_os("RMI_PARITY_TRACE")
        .map(PathBuf::from)
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "RMI_PARITY_TRACE is unset"))
}

fn selected(name: &str) -> bool {
    std::env::var("RMI_PARITY_FILTER")
        .ok()
        .map(|filter| filter.split(',').any(|candidate| candidate == name))
        .unwrap_or(true)
}

fn append(value: &serde_json::Value) -> io::Result<()> {
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(trace_path()?)?;
    serde_json::to_writer(&mut file, value)?;
    file.write_all(b"\n")
}

fn full_f32(name: &str, values: &[f32]) -> io::Result<PathBuf> {
    let path = PathBuf::from(format!("{}.{}.f32", trace_path()?.display(), name));
    let mut file = std::fs::File::create(&path)?;
    for value in values {
        file.write_all(&value.to_le_bytes())?;
    }
    Ok(path)
}

pub fn checkpoint(
    name: &str,
    layer: Option<usize>,
    shape: &[usize],
    values: &[f32],
) -> io::Result<Option<PathBuf>> {
    if !selected(name) {
        return Ok(None);
    }
    let expected_len = shape.iter().try_fold(1usize, |length, dimension| {
        length
            .checked_mul(*dimension)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "checkpoint shape overflow"))
    })?;
    if expected_len != values.len() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "checkpoint {name} shape has {expected_len} values, buffer has {}",
                values.len()
            ),
        ));
    }
    let sum = values.iter().map(|value| f64::from(*value)).sum::<f64>();
    let min = values.iter().copied().fold(f32::INFINITY, f32::min);
    let max = values.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    let head: Vec<f32> = values.iter().copied().take(8).collect();
    let mut tail: Vec<f32> = values.iter().rev().copied().take(8).collect();
    tail.reverse();
    append(&json!({
        "name": name,
        "layer": layer,
        "shape": shape,
        "len": values.len(),
        "finite": values.iter().all(|value| value.is_finite()),
        "sum": sum,
        "min": min,
        "max": max,
        "head": head,
        "tail": tail,
    }))?;
    full_f32(name, values).map(Some)
}

pub fn token_ids(name: &str, values: &[u32]) -> io::Result<()> {
    if selected(name) {
        append(&json!({ "name": name, "token_ids": values }))?;
    }
    Ok(())
}

pub fn usize_values(name: &str, shape: &[usize], values: &[usize]) -> io::Result<()> {
    if selected(name) {
        let expected_len = shape.iter().try_fold(1usize, |length, dimension| {
            length.checked_mul(*dimension).ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidInput, "usize trace shape overflow")
            })
        })?;
        if expected_len != values.len() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!(
                    "trace {name} expected {expected_len} values, got {}",
                    values.len()
                ),
            ));
        }
        append(&json!({ "name": name, "shape": shape, "usize_values": values }))?;
    }
    Ok(())
}

pub fn bool_values(name: &str, values: &[bool]) -> io::Result<()> {
    if selected(name) {
        append(&json!({ "name": name, "bool_values": values }))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checkpoint_schema_has_deterministic_stats_and_names() {
        let path = std::env::temp_dir().join(format!(
            "rmi-parity-trace-{}-{}.jsonl",
            std::process::id(),
            line!()
        ));
        let _ = std::fs::remove_file(&path);
        std::env::set_var("RMI_PARITY_TRACE", &path);
        let binary = checkpoint("Qcur_normed-0", Some(0), &[2], &[1.0, 3.0])
            .unwrap()
            .unwrap();
        usize_values("qwen35.positions", &[1, 4], &[7, 7, 7, 0]).unwrap();
        bool_values("qwen35.is_recurrent", &[true, false]).unwrap();
        let records: Vec<serde_json::Value> = std::fs::read_to_string(&path)
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        let value = &records[0];
        assert_eq!(value["name"], "Qcur_normed-0");
        assert_eq!(value["layer"], 0);
        assert_eq!(value["shape"], json!([2]));
        assert_eq!(value["sum"], 4.0);
        assert_eq!(value["min"], 1.0);
        assert_eq!(value["max"], 3.0);
        assert_eq!(records[1]["shape"], json!([1, 4]));
        assert_eq!(records[1]["usize_values"], json!([7, 7, 7, 0]));
        assert_eq!(records[2]["bool_values"], json!([true, false]));
        assert_eq!(std::fs::read(&binary).unwrap().len(), 8);
        std::fs::remove_file(binary).unwrap();
        std::fs::remove_file(path).unwrap();
        std::env::remove_var("RMI_PARITY_TRACE");
    }
}
