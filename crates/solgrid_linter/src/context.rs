//! Lint context — everything a rule needs to inspect source code.

use solgrid_config::Config;
use std::path::Path;

/// Context provided to each rule during linting.
///
/// Contains the source text, config, and file path.
/// Rules use this to inspect the source and produce diagnostics.
pub struct LintContext<'a> {
    /// The original source text.
    pub source: &'a str,
    /// The file path being linted.
    pub path: &'a Path,
    /// The active configuration.
    pub config: &'a Config,
}

impl<'a> LintContext<'a> {
    /// Create a new lint context for the given source, path, and config.
    pub fn new(source: &'a str, path: &'a Path, config: &'a Config) -> Self {
        Self {
            source,
            path,
            config,
        }
    }

    /// Get a line and column number for a byte offset.
    pub fn line_col(&self, offset: usize) -> (usize, usize) {
        let mut line = 1;
        let mut col = 1;
        for (i, ch) in self.source.char_indices() {
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

    /// Get the line number for a byte offset (1-based).
    pub fn line_number(&self, offset: usize) -> usize {
        self.line_col(offset).0
    }

    /// Get the text of a specific line (1-based).
    pub fn line_text(&self, line: usize) -> Option<&str> {
        self.source.lines().nth(line - 1)
    }
}
