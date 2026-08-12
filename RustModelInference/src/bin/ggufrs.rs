use rust_model_inference::{export_ggufrs, ExportOptions};
use std::collections::VecDeque;
use std::ffi::{OsStr, OsString};
use std::path::PathBuf;

const USAGE: &str = "Usage: ggufrs export --llm <model.gguf> [--mmproj <mmproj.gguf>] --output <model.ggufrs> [--overwrite]";

#[derive(Debug, PartialEq, Eq)]
enum Command {
    Export {
        llm: PathBuf,
        mmproj: Option<PathBuf>,
        output: PathBuf,
        overwrite: bool,
    },
}

fn take_value(args: &mut VecDeque<OsString>, flag: &str) -> Result<PathBuf, String> {
    let value = args
        .pop_front()
        .ok_or_else(|| format!("Missing value for {flag}"))?;
    if value.as_os_str().as_encoded_bytes().starts_with(b"--") {
        return Err(format!("Missing value for {flag}"));
    }
    Ok(PathBuf::from(value))
}

fn parse_args(args: impl IntoIterator<Item = OsString>) -> Result<Command, String> {
    let mut args: VecDeque<OsString> = args.into_iter().collect();
    if args.pop_front().as_deref() != Some(OsStr::new("export")) {
        return Err("Expected the export command".into());
    }
    let mut llm = None;
    let mut mmproj = None;
    let mut output = None;
    let mut overwrite = false;
    while let Some(flag) = args.pop_front() {
        if flag == OsStr::new("--llm") {
            if llm.is_some() {
                return Err("Duplicate --llm".into());
            }
            llm = Some(take_value(&mut args, "--llm")?);
        } else if flag == OsStr::new("--mmproj") {
            if mmproj.is_some() {
                return Err("Duplicate --mmproj".into());
            }
            mmproj = Some(take_value(&mut args, "--mmproj")?);
        } else if flag == OsStr::new("--output") {
            if output.is_some() {
                return Err("Duplicate --output".into());
            }
            output = Some(take_value(&mut args, "--output")?);
        } else if flag == OsStr::new("--overwrite") {
            if overwrite {
                return Err("Duplicate --overwrite".into());
            }
            overwrite = true;
        } else {
            return Err(format!("Unknown argument {:?}", flag));
        }
    }
    Ok(Command::Export {
        llm: llm.ok_or("Missing required --llm")?,
        mmproj,
        output: output.ok_or("Missing required --output")?,
        overwrite,
    })
}

fn main() {
    let command = parse_args(std::env::args_os().skip(1)).unwrap_or_else(|error| {
        eprintln!("{error}\n{USAGE}");
        std::process::exit(2);
    });
    match command {
        Command::Export {
            llm,
            mmproj,
            output,
            overwrite,
        } => export_ggufrs(
            &output,
            &llm,
            mmproj.as_deref(),
            ExportOptions { overwrite },
        )
        .unwrap_or_else(|error| {
            eprintln!("Failed to export {}: {error}", output.display());
            std::process::exit(1);
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args<'a>(values: &'a [&'a str]) -> impl Iterator<Item = OsString> + 'a {
        values.iter().map(OsString::from)
    }

    #[test]
    fn parses_export_with_optional_mmproj_and_overwrite() {
        assert_eq!(
            parse_args(args(&[
                "export",
                "--llm",
                "model.gguf",
                "--mmproj",
                "mmproj.gguf",
                "--output",
                "model.ggufrs",
                "--overwrite",
            ]))
            .unwrap(),
            Command::Export {
                llm: "model.gguf".into(),
                mmproj: Some("mmproj.gguf".into()),
                output: "model.ggufrs".into(),
                overwrite: true,
            }
        );
    }

    #[test]
    fn export_arguments_are_strict_and_complete() {
        for invalid in [
            vec![],
            vec!["inspect"],
            vec!["export"],
            vec!["export", "--llm"],
            vec!["export", "--llm", "a.gguf"],
            vec!["export", "--output", "a.ggufrs"],
            vec!["export", "--llm", "--overwrite", "--output", "x.ggufrs"],
            vec!["export", "--llm", "a.gguf", "--output", "--output"],
            vec![
                "export", "--llm", "a.gguf", "--llm", "b.gguf", "--output", "x.ggufrs",
            ],
            vec!["export", "--llm", "a.gguf", "--output", "x.ggufrs", "extra"],
            vec![
                "export",
                "--llm",
                "a.gguf",
                "--output",
                "x.ggufrs",
                "--unknown",
            ],
        ] {
            assert!(parse_args(args(&invalid)).is_err(), "accepted {invalid:?}");
        }
    }
}
