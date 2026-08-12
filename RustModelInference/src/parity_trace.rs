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

fn occurrence(name: &str) -> io::Result<usize> {
    let path = trace_path()?;
    let contents = match std::fs::read_to_string(path) {
        Ok(contents) => contents,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(0),
        Err(error) => return Err(error),
    };
    contents.lines().try_fold(0usize, |count, line| {
        let value: serde_json::Value = serde_json::from_str(line)?;
        Ok(count + usize::from(value["name"] == name))
    })
}

fn full_f32(name: &str, occurrence: usize, values: &[f32]) -> io::Result<PathBuf> {
    let trace = trace_path()?;
    let path = if occurrence == 0 {
        PathBuf::from(format!("{}.{}.f32", trace.display(), name))
    } else {
        PathBuf::from(format!("{}.{}.{}.f32", trace.display(), name, occurrence))
    };
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
    let occurrence = occurrence(name)?;
    let binary_path = full_f32(name, occurrence, values)?;
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
        "occurrence": occurrence,
        "binary_path": binary_path,
    }))?;
    Ok(Some(binary_path))
}

fn report_message<T>(result: io::Result<T>) -> Option<String> {
    result.err().and_then(|error| {
        std::env::var_os("RMI_PARITY_TRACE").map(|_| format!("parity trace error: {error}"))
    })
}

pub fn report<T>(result: io::Result<T>) {
    if let Some(message) = report_message(result) {
        eprintln!("{message}");
    }
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
    use std::sync::Mutex;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn checkpoint_schema_has_deterministic_stats_and_names() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|poison| poison.into_inner());
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

    #[test]
    fn repeated_checkpoints_keep_every_full_buffer_and_record_its_sidecar() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|poison| poison.into_inner());
        let path = std::env::temp_dir().join(format!(
            "rmi-parity-trace-{}-{}.jsonl",
            std::process::id(),
            line!()
        ));
        let _ = std::fs::remove_file(&path);
        std::env::set_var("RMI_PARITY_TRACE", &path);

        let first = checkpoint("result_output", None, &[2], &[1.0, 2.0])
            .unwrap()
            .unwrap();
        let second = checkpoint("result_output", None, &[2], &[3.0, 4.0])
            .unwrap()
            .unwrap();
        let records: Vec<serde_json::Value> = std::fs::read_to_string(&path)
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();

        assert_eq!(
            first,
            PathBuf::from(format!("{}.result_output.f32", path.display()))
        );
        assert_ne!(first, second);
        assert_eq!(records[0]["name"], "result_output");
        assert_eq!(records[1]["name"], "result_output");
        assert_eq!(records[0]["occurrence"], 0);
        assert_eq!(records[1]["occurrence"], 1);
        assert_eq!(records[0]["binary_path"], first.to_string_lossy().as_ref());
        assert_eq!(records[1]["binary_path"], second.to_string_lossy().as_ref());
        assert_eq!(
            std::fs::read(&first).unwrap(),
            [1.0f32.to_le_bytes(), 2.0f32.to_le_bytes()].concat()
        );
        assert_eq!(
            std::fs::read(&second).unwrap(),
            [3.0f32.to_le_bytes(), 4.0f32.to_le_bytes()].concat()
        );

        std::fs::remove_file(first).unwrap();
        std::fs::remove_file(second).unwrap();
        std::fs::remove_file(path).unwrap();
        std::env::remove_var("RMI_PARITY_TRACE");
    }

    #[test]
    fn reporting_is_silent_only_when_trace_path_is_unset() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|poison| poison.into_inner());
        std::env::remove_var("RMI_PARITY_TRACE");
        assert_eq!(
            report_message(Err::<(), _>(io::Error::new(
                io::ErrorKind::NotFound,
                "RMI_PARITY_TRACE is unset",
            ))),
            None
        );

        std::env::set_var("RMI_PARITY_TRACE", "configured.jsonl");
        assert_eq!(
            report_message(Err::<(), _>(io::Error::new(
                io::ErrorKind::InvalidInput,
                "configured failure",
            )))
            .as_deref(),
            Some("parity trace error: configured failure")
        );
        std::env::remove_var("RMI_PARITY_TRACE");
    }
}
