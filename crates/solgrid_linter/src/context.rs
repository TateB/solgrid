//! Lint context — everything a rule needs to inspect source code.

use crate::source_utils::{is_in_non_code_region, scan_source_regions, SourceRegion};
use solgrid_config::Config;
use std::cell::OnceCell;
use std::path::{Path, PathBuf};

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
    /// Project remappings (prefix → target directory).
    pub remappings: &'a [(String, PathBuf)],
    /// Lazily-computed non-code regions (comments, strings).
    source_regions: OnceCell<Vec<SourceRegion>>,
}

impl<'a> LintContext<'a> {
    /// Create a new lint context for the given source, path, config, and remappings.
    pub fn new(
        source: &'a str,
        path: &'a Path,
        config: &'a Config,
        remappings: &'a [(String, PathBuf)],
    ) -> Self {
        Self {
            source,
            path,
            config,
            remappings,
            source_regions: OnceCell::new(),
        }
    }

    /// Check whether a byte offset falls inside a comment or string literal.
    ///
    /// The underlying region scan is computed once (on first call) and cached
    /// for the lifetime of this context. Subsequent calls are O(log n) lookups.
    pub fn is_in_comment_or_string(&self, pos: usize) -> bool {
        let regions = self
            .source_regions
            .get_or_init(|| scan_source_regions(self.source));
        is_in_non_code_region(regions, pos)
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
