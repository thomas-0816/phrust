pub(super) fn normalize_repository_paths(detail: &mut String) {
    // Harness diagnostics for binary PHPT inputs include the checkout root.
    // Normalize stable repository-relative paths before hashing so sibling
    // worktrees produce the same BORK fingerprint.
    for marker in ["/third_party/php-src/", "/tests/phpt/generated/"] {
        let mut search_from = 0;
        while let Some(relative_start) = detail[search_from..].find(marker) {
            let marker_start = search_from + relative_start;
            let prefix_start = detail[..marker_start]
                .rfind(|ch: char| ch.is_ascii_whitespace() || matches!(ch, '=' | '"' | '`'))
                .map(|index| index + 1)
                .unwrap_or(0);
            detail.replace_range(prefix_start..marker_start, "<repo>");
            search_from = prefix_start + "<repo>".len() + marker.len();
        }
    }
}

pub(super) fn normalize_work_directory_paths(detail: &mut String) {
    // PHPT_WORK_DIR is configurable. Keep the conventional `phpt-work`
    // prefix recognizable when callers use an isolated sibling such as
    // `phpt-work-one-worker`; otherwise the run directory leaks into every
    // failure fingerprint and makes an unchanged failure look new.
    for marker in ["/target/phpt-work", "target/phpt-work"] {
        while let Some(marker_start) = detail.find(marker) {
            let prefix_start = detail[..marker_start]
                .rfind(|ch: char| ch.is_ascii_whitespace() || matches!(ch, '=' | '"' | '`'))
                .map(|index| index + 1)
                .unwrap_or(0);
            let Some(test_php_offset) = detail[marker_start..].find("test.php") else {
                break;
            };
            let end = marker_start + test_php_offset + "test.php".len();
            detail.replace_range(prefix_start..end, "<phpt-test.php>");
        }
    }
    for marker in ["/target/phpt-work", "target/phpt-work"] {
        while let Some(marker_start) = detail.find(marker) {
            let prefix_start = detail[..marker_start]
                .rfind(|ch: char| ch.is_ascii_whitespace() || matches!(ch, '=' | '"' | '`'))
                .map(|index| index + 1)
                .unwrap_or(0);
            let end = detail[marker_start..]
                .find(|ch: char| ch.is_ascii_whitespace() || matches!(ch, '"' | '`'))
                .map(|offset| marker_start + offset)
                .unwrap_or(detail.len());
            detail.replace_range(prefix_start..end, "<phpt-work-path>");
        }
    }
}
