//! Formata o [`Report`] como texto para leitura humana ou JSON para scripts.

use std::io::{self, Write};

use serde::Serialize;

use crate::size::format_size;
use crate::{Report, Summary};

/// Um bloco por grupo e um resumo no fim.
pub fn write_text(out: &mut impl Write, report: &Report) -> io::Result<()> {
    for group in &report.groups {
        let hex = group.hash.to_hex();
        writeln!(
            out,
            "{} copies of {} ({} reclaimable)  blake3:{}",
            group.paths.len(),
            format_size(group.size),
            format_size(group.reclaimable_bytes()),
            &hex.as_str()[..16],
        )?;
        for path in &group.paths {
            writeln!(out, "  {}", path.display())?;
        }
        writeln!(out)?;
    }

    let summary = report.summary();
    let scanned = plural(summary.scanned_files, "file", "files");
    if summary.duplicate_groups == 0 {
        writeln!(out, "Scanned {scanned}: no duplicates found.")?;
    } else {
        writeln!(
            out,
            "Scanned {scanned}: {}, {}, {} reclaimable.",
            plural(
                summary.duplicate_groups,
                "duplicate group",
                "duplicate groups"
            ),
            plural(
                summary.redundant_files,
                "redundant copy",
                "redundant copies"
            ),
            format_size(summary.reclaimable_bytes),
        )?;
    }
    if summary.errors > 0 {
        writeln!(
            out,
            "{} could not be read (see warnings).",
            plural(summary.errors, "item", "items")
        )?;
    }
    Ok(())
}

#[derive(Serialize)]
struct JsonReport<'a> {
    summary: Summary,
    groups: Vec<JsonGroup>,
    errors: Vec<JsonError<'a>>,
}

#[derive(Serialize)]
struct JsonGroup {
    size: u64,
    reclaimable_bytes: u64,
    hash: String,
    paths: Vec<String>,
}

#[derive(Serialize)]
struct JsonError<'a> {
    path: Option<String>,
    message: &'a str,
}

/// JSON estável para scripts. Caminhos que não são UTF-8 válido saem com
/// conversão com perda, em vez de derrubar a serialização.
pub fn write_json(out: &mut impl Write, report: &Report) -> io::Result<()> {
    let json = JsonReport {
        summary: report.summary(),
        groups: report
            .groups
            .iter()
            .map(|group| JsonGroup {
                size: group.size,
                reclaimable_bytes: group.reclaimable_bytes(),
                hash: group.hash.to_hex().to_string(),
                paths: group
                    .paths
                    .iter()
                    .map(|path| path.to_string_lossy().into_owned())
                    .collect(),
            })
            .collect(),
        errors: report
            .errors
            .iter()
            .map(|error| JsonError {
                path: error
                    .path
                    .as_ref()
                    .map(|path| path.to_string_lossy().into_owned()),
                message: &error.message,
            })
            .collect(),
    };
    serde_json::to_writer_pretty(&mut *out, &json)?;
    writeln!(out)
}

fn plural(count: usize, singular: &str, plural: &str) -> String {
    format!("{count} {}", if count == 1 { singular } else { plural })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::DuplicateGroup;

    fn sample() -> Report {
        Report {
            groups: vec![DuplicateGroup {
                size: 2048,
                hash: blake3::hash(b"sample"),
                paths: vec!["a/one.bin".into(), "b/two.bin".into()],
            }],
            errors: Vec::new(),
            scanned_files: 5,
        }
    }

    #[test]
    fn text_lists_each_group_and_a_summary() {
        let mut out = Vec::new();
        write_text(&mut out, &sample()).unwrap();
        let text = String::from_utf8(out).unwrap();

        assert!(text.starts_with("2 copies of 2.0 KiB (2.0 KiB reclaimable)  blake3:"));
        assert!(text.contains("one.bin"));
        assert!(text.ends_with(
            "Scanned 5 files: 1 duplicate group, 1 redundant copy, 2.0 KiB reclaimable.\n"
        ));
    }

    #[test]
    fn text_reports_when_nothing_was_found() {
        let mut out = Vec::new();
        let report = Report {
            scanned_files: 1,
            ..Report::default()
        };
        write_text(&mut out, &report).unwrap();
        assert_eq!(
            String::from_utf8(out).unwrap(),
            "Scanned 1 file: no duplicates found.\n"
        );
    }

    #[test]
    fn json_contains_full_hash_and_summary() {
        let mut out = Vec::new();
        write_json(&mut out, &sample()).unwrap();
        let json: serde_json::Value = serde_json::from_slice(&out).unwrap();

        assert_eq!(json["summary"]["reclaimable_bytes"], 2048);
        assert_eq!(json["summary"]["duplicate_groups"], 1);
        assert_eq!(json["groups"][0]["hash"].as_str().unwrap().len(), 64);
        assert_eq!(json["groups"][0]["paths"].as_array().unwrap().len(), 2);
        assert!(json["errors"].as_array().unwrap().is_empty());
    }
}
