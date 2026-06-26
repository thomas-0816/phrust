#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PhptDocument {
    pub sections: Vec<PhptSection>,
    pub diagnostics: Vec<PhptDiagnostic>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PhptSection {
    pub name: String,
    pub body: String,
    pub header_range: SourceRange,
    pub body_range: SourceRange,
    pub supported: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SourceRange {
    pub start: usize,
    pub end: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PhptDiagnostic {
    pub id: &'static str,
    pub message: String,
    pub range: SourceRange,
}

pub const PHPT_SUPPORTED_SECTIONS: &[&str] = &[
    "TEST",
    "DESCRIPTION",
    "CREDITS",
    "EXTENSIONS",
    "FLAKY",
    "SKIPIF",
    "XFAIL",
    "XLEAK",
    "CONFLICTS",
    "INI",
    "ENV",
    "ARGS",
    "STDIN",
    "CAPTURE_STDIO",
    "GET",
    "POST",
    "POST_RAW",
    "GZIP_POST",
    "DEFLATE_POST",
    "PUT",
    "COOKIE",
    "CGI",
    "PHPDBG",
    "FILE",
    "FILEEOF",
    "FILE_EXTERNAL",
    "EXPECT",
    "EXPECTF",
    "EXPECTREGEX",
    "EXPECT_EXTERNAL",
    "EXPECTF_EXTERNAL",
    "EXPECTREGEX_EXTERNAL",
    "EXPECTHEADERS",
    "CLEAN",
    "WHITESPACE_SENSITIVE",
    "REDIRECTTEST",
];

pub fn parse_phpt(source: &str) -> PhptDocument {
    let mut sections = Vec::new();
    let mut diagnostics = Vec::new();
    let mut current = None::<OpenSection>;
    let mut offset = 0usize;
    let mut saw_any_marker = false;

    for line in source.split_inclusive('\n') {
        let line_start = offset;
        let line_end = offset + line.len();
        if let Some(name) = section_marker(line) {
            saw_any_marker = true;
            if let Some(open) = current.take() {
                sections.push(close_section(open, line_start, source));
            } else if !source[..line_start].trim().is_empty() {
                diagnostics.push(PhptDiagnostic {
                    id: "PHPT_TEXT_BEFORE_FIRST_SECTION",
                    message: "PHPT contains text before the first section".to_string(),
                    range: SourceRange {
                        start: 0,
                        end: line_start,
                    },
                });
            }
            let supported = is_supported_section(&name);
            if !supported {
                diagnostics.push(PhptDiagnostic {
                    id: "PHPT_UNSUPPORTED_SECTION",
                    message: format!("unsupported PHPT section `{name}`"),
                    range: SourceRange {
                        start: line_start,
                        end: line_end,
                    },
                });
            }
            current = Some(OpenSection {
                name,
                header_range: SourceRange {
                    start: line_start,
                    end: line_end,
                },
                body_start: line_end,
                supported,
            });
        }
        offset = line_end;
    }

    if let Some(open) = current.take() {
        sections.push(close_section(open, source.len(), source));
    }

    if !saw_any_marker {
        diagnostics.push(PhptDiagnostic {
            id: "PHPT_NO_SECTIONS",
            message: "PHPT contains no section headers".to_string(),
            range: SourceRange {
                start: 0,
                end: source.len(),
            },
        });
    }
    if !sections.iter().any(|section| section.name == "TEST") {
        diagnostics.push(PhptDiagnostic {
            id: "PHPT_MISSING_TEST",
            message: "PHPT is missing a TEST section".to_string(),
            range: SourceRange {
                start: 0,
                end: source.len().min(1),
            },
        });
    }
    if !sections.iter().any(|section| {
        section.name == "FILE" || section.name == "FILEEOF" || section.name == "FILE_EXTERNAL"
    }) {
        diagnostics.push(PhptDiagnostic {
            id: "PHPT_MISSING_FILE",
            message: "PHPT is missing FILE, FILEEOF, or FILE_EXTERNAL".to_string(),
            range: SourceRange {
                start: 0,
                end: source.len().min(1),
            },
        });
    }
    if !sections.iter().any(|section| {
        matches!(
            section.name.as_str(),
            "EXPECT"
                | "EXPECTF"
                | "EXPECTREGEX"
                | "EXPECT_EXTERNAL"
                | "EXPECTF_EXTERNAL"
                | "EXPECTREGEX_EXTERNAL"
        )
    }) {
        diagnostics.push(PhptDiagnostic {
            id: "PHPT_MISSING_EXPECTATION",
            message: "PHPT is missing an expectation section".to_string(),
            range: SourceRange {
                start: 0,
                end: source.len().min(1),
            },
        });
    }

    PhptDocument {
        sections,
        diagnostics,
    }
}

fn close_section(open: OpenSection, body_end: usize, source: &str) -> PhptSection {
    PhptSection {
        name: open.name,
        body: source[open.body_start..body_end].to_string(),
        header_range: open.header_range,
        body_range: SourceRange {
            start: open.body_start,
            end: body_end,
        },
        supported: open.supported,
    }
}

#[derive(Debug)]
struct OpenSection {
    name: String,
    header_range: SourceRange,
    body_start: usize,
    supported: bool,
}

fn section_marker(line: &str) -> Option<String> {
    let trimmed = line.trim_end_matches(['\r', '\n']);
    let inner = trimmed.strip_prefix("--")?.strip_suffix("--")?;
    if inner.is_empty()
        || !inner
            .chars()
            .all(|ch| ch.is_ascii_uppercase() || ch == '_' || ch.is_ascii_digit())
    {
        return None;
    }
    Some(inner.to_string())
}

fn is_supported_section(name: &str) -> bool {
    PHPT_SUPPORTED_SECTIONS.contains(&name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preserves_body_line_endings_and_spans() {
        let source = "--TEST--\r\nhello\r\n--FILE--\r\n<?php echo 1;\r\n--EXPECT--\r\n1\r\n";

        let document = parse_phpt(source);

        assert!(
            document.diagnostics.is_empty(),
            "{:?}",
            document.diagnostics
        );
        assert_eq!(document.sections[0].name, "TEST");
        assert_eq!(document.sections[0].body, "hello\r\n");
        assert_eq!(
            &source[document.sections[1].body_range.start..document.sections[1].body_range.end],
            "<?php echo 1;\r\n"
        );
    }

    #[test]
    fn parses_supported_target_sections() {
        let mut source = String::new();
        for section in PHPT_SUPPORTED_SECTIONS {
            source.push_str("--");
            source.push_str(section);
            source.push_str("--\nbody\n");
        }

        let document = parse_phpt(&source);

        assert_eq!(document.sections.len(), PHPT_SUPPORTED_SECTIONS.len());
        assert!(document.sections.iter().all(|section| section.supported));
        assert!(
            document.diagnostics.is_empty(),
            "{:?}",
            document.diagnostics
        );
    }

    #[test]
    fn reports_unsupported_sections_as_bork_candidates() {
        let source = "--TEST--\nname\n--FILE--\n<?php\n--UNKNOWN--\nx\n--EXPECT--\n";

        let document = parse_phpt(source);

        assert!(document.diagnostics.iter().any(|diagnostic| {
            diagnostic.id == "PHPT_UNSUPPORTED_SECTION" && diagnostic.message.contains("UNKNOWN")
        }));
    }

    #[test]
    fn reports_missing_required_sections() {
        let document = parse_phpt("not a phpt\n");

        let ids = document
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.id)
            .collect::<Vec<_>>();
        assert!(ids.contains(&"PHPT_NO_SECTIONS"));
        assert!(ids.contains(&"PHPT_MISSING_TEST"));
        assert!(ids.contains(&"PHPT_MISSING_FILE"));
        assert!(ids.contains(&"PHPT_MISSING_EXPECTATION"));
    }
}
