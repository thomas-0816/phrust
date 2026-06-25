use regex::Regex;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExpectationKind {
    Expect,
    ExpectF,
    ExpectRegex,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MatchOutcome {
    pub matched: bool,
    pub diff: Option<ExpectationDiff>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExpectationDiff {
    pub kind: ExpectationKind,
    pub message: String,
    pub first_mismatch: Option<usize>,
    pub expected_excerpt: String,
    pub actual_excerpt: String,
}

pub fn match_expectation(kind: ExpectationKind, expected: &str, actual: &str) -> MatchOutcome {
    let matched = match kind {
        ExpectationKind::Expect => expected == actual,
        ExpectationKind::ExpectF => match expectf_to_regex(expected) {
            Ok(regex) => regex.is_match(actual),
            Err(error) => {
                return MatchOutcome {
                    matched: false,
                    diff: Some(ExpectationDiff {
                        kind,
                        message: error,
                        first_mismatch: None,
                        expected_excerpt: excerpt(expected, 0),
                        actual_excerpt: excerpt(actual, 0),
                    }),
                };
            }
        },
        ExpectationKind::ExpectRegex => match anchored_regex(expected) {
            Ok(regex) => regex.is_match(actual),
            Err(error) => {
                return MatchOutcome {
                    matched: false,
                    diff: Some(ExpectationDiff {
                        kind,
                        message: format!("invalid EXPECTREGEX pattern: {error}"),
                        first_mismatch: None,
                        expected_excerpt: excerpt(expected, 0),
                        actual_excerpt: excerpt(actual, 0),
                    }),
                };
            }
        },
    };
    if matched {
        MatchOutcome {
            matched: true,
            diff: None,
        }
    } else {
        let mismatch = first_mismatch(expected, actual);
        MatchOutcome {
            matched: false,
            diff: Some(ExpectationDiff {
                kind,
                message: "output did not match expectation".to_string(),
                first_mismatch: mismatch,
                expected_excerpt: excerpt(expected, mismatch.unwrap_or(0)),
                actual_excerpt: excerpt(actual, mismatch.unwrap_or(0)),
            }),
        }
    }
}

pub fn expectf_to_regex(pattern: &str) -> Result<Regex, String> {
    let mut out = String::from("(?s)\\A");
    let mut index = 0usize;
    while index < pattern.len() {
        let rest = &pattern[index..];
        if rest.starts_with("%r") {
            if let Some(end) = pattern[index + 2..].find("%r") {
                out.push_str("(?:");
                out.push_str(&pattern[index + 2..index + 2 + end]);
                out.push(')');
                index += end + 4;
                continue;
            }
        }
        if let Some(regex) = expectf_placeholder(rest) {
            out.push_str(regex.pattern);
            index += regex.width;
        } else {
            let ch = rest
                .chars()
                .next()
                .ok_or_else(|| "invalid UTF-8 boundary in EXPECTF".to_string())?;
            out.push_str(&regex::escape(&ch.to_string()));
            index += ch.len_utf8();
        }
    }
    out.push_str("\\z");
    Regex::new(&out).map_err(|error| format!("invalid EXPECTF pattern: {error}"))
}

fn anchored_regex(pattern: &str) -> Result<Regex, regex::Error> {
    let pattern = normalize_pcre_regex(pattern);
    match Regex::new(&format!("(?s)\\A(?:{pattern})\\z")) {
        Ok(regex) => Ok(regex),
        Err(_) => Regex::new(&format!(
            "(?s)\\A(?:{})\\z",
            escape_pcre_literal_braces(&pattern)
        )),
    }
}

fn normalize_pcre_regex(pattern: &str) -> String {
    let mut out = String::with_capacity(pattern.len());
    let mut chars = pattern.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\\' {
            match chars.next() {
                Some('0') => out.push_str("\\x00"),
                Some(next) => {
                    out.push('\\');
                    out.push(next);
                }
                None => out.push('\\'),
            }
        } else {
            out.push(ch);
        }
    }
    out
}

fn escape_pcre_literal_braces(pattern: &str) -> String {
    let mut out = String::with_capacity(pattern.len());
    let mut chars = pattern.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '\\' => {
                out.push(ch);
                if let Some(next) = chars.next() {
                    out.push(next);
                }
            }
            '{' if looks_like_quantifier(&chars) => {
                out.push(ch);
                for next in chars.by_ref() {
                    out.push(next);
                    if next == '}' {
                        break;
                    }
                }
            }
            '{' => out.push_str("\\{"),
            '}' => out.push_str("\\}"),
            _ => out.push(ch),
        }
    }
    out
}

fn looks_like_quantifier<I>(chars: &I) -> bool
where
    I: Iterator<Item = char> + Clone,
{
    let mut clone = chars.clone();
    let mut saw_digit = false;
    while let Some(ch) = clone.next() {
        match ch {
            '0'..='9' => saw_digit = true,
            ',' => {}
            '}' => return saw_digit,
            _ => return false,
        }
    }
    false
}

struct Placeholder {
    pattern: &'static str,
    width: usize,
}

fn expectf_placeholder(rest: &str) -> Option<Placeholder> {
    if rest.starts_with("%unicode|string%") {
        return Some(Placeholder {
            pattern: "(?:unicode|string)",
            width: "%unicode|string%".len(),
        });
    }
    let mut chars = rest.chars();
    if chars.next()? != '%' {
        return None;
    }
    let placeholder = chars.next()?;
    let pattern = match placeholder {
        '%' => "%",
        'e' => {
            if cfg!(windows) {
                "\\\\"
            } else {
                "/"
            }
        }
        's' => "[^\\r\\n]+",
        'S' => "[^\\r\\n]*",
        'd' => "\\d+",
        'i' => "[+-]?\\d+",
        'f' => "[+-]?(?:\\d+\\.\\d*|\\d*\\.\\d+|\\d+)(?:[Ee][+-]?\\d+)?",
        'x' => "[0-9A-Fa-f]+",
        'w' => "\\s*",
        'a' => ".+?",
        'A' => ".*?",
        'c' => ".",
        '0' => "\\x00",
        _ => return None,
    };
    Some(Placeholder { pattern, width: 2 })
}

fn first_mismatch(expected: &str, actual: &str) -> Option<usize> {
    let expected_bytes = expected.as_bytes();
    let actual_bytes = actual.as_bytes();
    let len = expected_bytes.len().min(actual_bytes.len());
    for index in 0..len {
        if expected_bytes[index] != actual_bytes[index] {
            return Some(index);
        }
    }
    if expected_bytes.len() == actual_bytes.len() {
        None
    } else {
        Some(len)
    }
}

fn excerpt(value: &str, index: usize) -> String {
    let start = index.saturating_sub(24);
    let end = (index + 80).min(value.len());
    let start = previous_char_boundary(value, start);
    let end = next_char_boundary(value, end);
    value[start..end].replace('\n', "\\n").replace('\r', "\\r")
}

fn previous_char_boundary(value: &str, mut index: usize) -> usize {
    while index > 0 && !value.is_char_boundary(index) {
        index -= 1;
    }
    index
}

fn next_char_boundary(value: &str, mut index: usize) -> usize {
    while index < value.len() && !value.is_char_boundary(index) {
        index += 1;
    }
    index
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_expect_matches_byte_for_byte() {
        assert!(match_expectation(ExpectationKind::Expect, "a\n", "a\n").matched);
        let outcome = match_expectation(ExpectationKind::Expect, "a\n", "a\r\n");
        assert!(!outcome.matched);
        assert_eq!(outcome.diff.unwrap().first_mismatch, Some(1));
    }

    #[test]
    fn expectf_supports_common_placeholders() {
        let expected = "int(%d) signed(%i) float(%f) hex(%x) ws%wtext %s %S %unicode|string%";
        let actual = "int(12) signed(-3) float(1.5e+2) hex(ff) ws \n\ttext hello WORD string";

        assert!(match_expectation(ExpectationKind::ExpectF, expected, actual).matched);
    }

    #[test]
    fn expectf_supports_any_placeholders() {
        assert!(match_expectation(ExpectationKind::ExpectF, "a%Aend", "a\nmiddle\nend").matched);
        assert!(match_expectation(ExpectationKind::ExpectF, "a%aend", "a\nx\nend").matched);
        assert!(!match_expectation(ExpectationKind::ExpectF, "a%aend", "aend").matched);
    }

    #[test]
    fn expectf_matches_php_run_tests_placeholders() {
        assert!(match_expectation(ExpectationKind::ExpectF, "a%0b", "a\0b").matched);
        assert!(
            match_expectation(ExpectationKind::ExpectF, "prefix%Ssuffix", "prefixsuffix").matched
        );
        assert!(match_expectation(ExpectationKind::ExpectF, "a%cb", "aXb").matched);
        assert!(!match_expectation(ExpectationKind::ExpectF, "a%cb", "aXYb").matched);

        let separator = std::path::MAIN_SEPARATOR.to_string();
        assert!(
            match_expectation(
                ExpectationKind::ExpectF,
                "root%edir",
                &format!("root{separator}dir")
            )
            .matched
        );
    }

    #[test]
    fn expectf_supports_raw_regex_regions() {
        assert!(
            match_expectation(ExpectationKind::ExpectF, "value=%r[a-z]+%r", "value=abc").matched
        );
        assert!(
            !match_expectation(ExpectationKind::ExpectF, "value=%r[a-z]+%r", "value=123").matched
        );
    }

    #[test]
    fn expectregex_is_anchored() {
        assert!(match_expectation(ExpectationKind::ExpectRegex, "a.+c", "abc").matched);
        assert!(!match_expectation(ExpectationKind::ExpectRegex, "a.+c", "zabc").matched);
    }

    #[test]
    fn expectregex_accepts_php_pcre_literal_braces() {
        let pattern = "array\\(1\\) {\n  \\[0\\]=>\n  string\\([0-9]+\\) \"ok\"\n}";
        let actual = "array(1) {\n  [0]=>\n  string(2) \"ok\"\n}";

        assert!(match_expectation(ExpectationKind::ExpectRegex, pattern, actual).matched);
    }

    #[test]
    fn expectregex_accepts_php_pcre_nul_escape() {
        let pattern = "string\\([0-9]+\\) \"[a-z \\0]*\"";
        let actual = "string(3) \"a\0b\"";

        assert!(match_expectation(ExpectationKind::ExpectRegex, pattern, actual).matched);
    }

    #[test]
    fn invalid_regex_returns_structured_diff() {
        let outcome = match_expectation(ExpectationKind::ExpectRegex, "(", "");

        assert!(!outcome.matched);
        assert!(
            outcome
                .diff
                .unwrap()
                .message
                .contains("invalid EXPECTREGEX")
        );
    }
}
