//! SARIF 2.1.0 output format.
//!
//! Produces OASIS SARIF JSON for integration with CodeQL, GitHub Advanced
//! Security, and other SARIF-consuming tools.

use serde::Serialize;
use solgrid_diagnostics::{FileResult, Severity};

/// Top-level SARIF report.
#[derive(Serialize)]
struct SarifReport {
    #[serde(rename = "$schema")]
    schema: &'static str,
    version: &'static str,
    runs: Vec<SarifRun>,
}

#[derive(Serialize)]
struct SarifRun {
    tool: SarifTool,
    results: Vec<SarifResult>,
}

#[derive(Serialize)]
struct SarifTool {
    driver: SarifDriver,
}

#[derive(Serialize)]
struct SarifDriver {
    name: &'static str,
    version: &'static str,
    #[serde(rename = "informationUri")]
    information_uri: &'static str,
}

#[derive(Serialize)]
struct SarifResult {
    #[serde(rename = "ruleId")]
    rule_id: String,
    level: &'static str,
    message: SarifMessage,
    locations: Vec<SarifLocation>,
}

#[derive(Serialize)]
struct SarifMessage {
    text: String,
}

#[derive(Serialize)]
struct SarifLocation {
    #[serde(rename = "physicalLocation")]
    physical_location: SarifPhysicalLocation,
}

#[derive(Serialize)]
struct SarifPhysicalLocation {
    #[serde(rename = "artifactLocation")]
    artifact_location: SarifArtifactLocation,
    region: SarifRegion,
}

#[derive(Serialize)]
struct SarifArtifactLocation {
    uri: String,
}

#[derive(Serialize)]
struct SarifRegion {
    #[serde(rename = "startLine")]
    start_line: usize,
    #[serde(rename = "startColumn")]
    start_column: usize,
}

/// Print results as SARIF 2.1.0 JSON.
pub fn print_results(results: &[FileResult]) {
    let mut sarif_results = Vec::new();

    for result in results {
        for diag in &result.diagnostics {
            let (line, col) = offset_to_line_col_from_file(&result.path, diag.span.start);

            let level = match diag.severity {
                Severity::Error => "error",
                Severity::Warning => "warning",
                Severity::Info => "note",
            };

            sarif_results.push(SarifResult {
                rule_id: diag.rule_id.clone(),
                level,
                message: SarifMessage {
                    text: diag.message.clone(),
                },
                locations: vec![SarifLocation {
                    physical_location: SarifPhysicalLocation {
                        artifact_location: SarifArtifactLocation {
                            uri: result.path.clone(),
                        },
                        region: SarifRegion {
                            start_line: line,
                            start_column: col,
                        },
                    },
                }],
            });
        }
    }

    let report = SarifReport {
        schema: "https://raw.githubusercontent.com/oasis-tcs/sarif-spec/main/sarif-2.1/schema/sarif-schema-2.1.0.json",
        version: "2.1.0",
        runs: vec![SarifRun {
            tool: SarifTool {
                driver: SarifDriver {
                    name: "solgrid",
                    version: env!("CARGO_PKG_VERSION"),
                    information_uri: "https://github.com/TateB/solgrid",
                },
            },
            results: sarif_results,
        }],
    };

    match serde_json::to_string_pretty(&report) {
        Ok(json) => println!("{json}"),
        Err(e) => eprintln!("Error serializing SARIF: {e}"),
    }
}

/// Read a file and compute line/col from byte offset.
fn offset_to_line_col_from_file(path: &str, offset: usize) -> (usize, usize) {
    if let Ok(source) = std::fs::read_to_string(path) {
        offset_to_line_col(&source, offset)
    } else {
        (1, 1)
    }
}

fn offset_to_line_col(source: &str, offset: usize) -> (usize, usize) {
    let mut line = 1;
    let mut col = 1;
    for (i, ch) in source.char_indices() {
        if i >= offset {
            break;
        }
        if ch == '\n' {
            line += 1;
            col = 1;
        } else {
            col += 1;
        }
    }
    (line, col)
}

#[cfg(test)]
mod tests {
    use super::*;
    use solgrid_diagnostics::{Diagnostic, Severity};

    #[test]
    fn test_offset_to_line_col_first_line() {
        let source = "pragma solidity ^0.8.0;";
        assert_eq!(offset_to_line_col(source, 0), (1, 1));
        assert_eq!(offset_to_line_col(source, 7), (1, 8));
    }

    #[test]
    fn test_offset_to_line_col_multiline() {
        let source = "line one\nline two\nline three";
        assert_eq!(offset_to_line_col(source, 0), (1, 1));
        assert_eq!(offset_to_line_col(source, 9), (2, 1));
        assert_eq!(offset_to_line_col(source, 18), (3, 1));
    }

    #[test]
    fn test_sarif_structure() {
        let results = vec![FileResult {
            path: "test.sol".to_string(),
            diagnostics: vec![Diagnostic::new(
                "security/tx-origin",
                "use of tx.origin",
                Severity::Error,
                10..20,
            )],
        }];

        // Build SARIF manually to verify structure
        let sarif_results: Vec<SarifResult> = results
            .iter()
            .flat_map(|r| {
                r.diagnostics.iter().map(|d| SarifResult {
                    rule_id: d.rule_id.clone(),
                    level: "error",
                    message: SarifMessage {
                        text: d.message.clone(),
                    },
                    locations: vec![SarifLocation {
                        physical_location: SarifPhysicalLocation {
                            artifact_location: SarifArtifactLocation {
                                uri: r.path.clone(),
                            },
                            region: SarifRegion {
                                start_line: 1,
                                start_column: 1,
                            },
                        },
                    }],
                })
            })
            .collect();

        let report = SarifReport {
            schema: "https://raw.githubusercontent.com/oasis-tcs/sarif-spec/main/sarif-2.1/schema/sarif-schema-2.1.0.json",
            version: "2.1.0",
            runs: vec![SarifRun {
                tool: SarifTool {
                    driver: SarifDriver {
                        name: "solgrid",
                        version: env!("CARGO_PKG_VERSION"),
                        information_uri: "https://github.com/TateB/solgrid",
                    },
                },
                results: sarif_results,
            }],
        };

        let json = serde_json::to_string(&report).unwrap();
        assert!(json.contains("\"version\":\"2.1.0\""));
        assert!(json.contains("security/tx-origin"));
        assert!(json.contains("test.sol"));
    }
}
