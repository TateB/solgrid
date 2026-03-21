//! Shared NatSpec parsing helpers used by best-practices rules.

/// Extract NatSpec comment block immediately preceding the given byte position.
/// Returns `None` if no NatSpec is found.
///
/// Supports both `///` line comments and `/** ... */` block comments.
pub(crate) fn extract_natspec(source: &str, item_start: usize) -> Option<String> {
    let before = &source[..item_start];

    let trimmed = before.trim_end();
    if trimmed.is_empty() {
        return None;
    }

    if trimmed.ends_with("*/") {
        if let Some(block_start) = trimmed.rfind("/**") {
            let block = &trimmed[block_start..];
            return Some(block.to_string());
        }
        return None;
    }

    let mut natspec_lines = Vec::new();
    for line in before.lines().rev() {
        let trimmed_line = line.trim();
        if trimmed_line.is_empty() {
            if natspec_lines.is_empty() {
                continue;
            }
            break;
        }
        if trimmed_line.starts_with("///") {
            natspec_lines.push(trimmed_line.to_string());
        } else {
            break;
        }
    }

    if natspec_lines.is_empty() {
        return None;
    }

    natspec_lines.reverse();
    Some(natspec_lines.join("\n"))
}

/// Parse `@param` names from a NatSpec string.
pub(crate) fn parse_natspec_params(natspec: &str) -> Vec<String> {
    let mut params = Vec::new();
    for line in natspec.lines() {
        let trimmed = line
            .trim()
            .trim_start_matches('/')
            .trim_start_matches('*')
            .trim();
        if let Some(rest) = trimmed.strip_prefix("@param") {
            let rest = rest.trim();
            if let Some(name) = rest.split_whitespace().next() {
                params.push(name.to_string());
            }
        }
    }
    params
}

/// Count `@return` tags in a NatSpec string.
pub(crate) fn count_natspec_returns(natspec: &str) -> usize {
    let mut count = 0;
    for line in natspec.lines() {
        let trimmed = line
            .trim()
            .trim_start_matches('/')
            .trim_start_matches('*')
            .trim();
        if trimmed.starts_with("@return") {
            count += 1;
        }
    }
    count
}
