/// Result of scanning a quoted string from scripting mode.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum StringScan {
    /// The string can be emitted as `T_CONSTANT_ENCAPSED_STRING`.
    Constant { len: usize, terminated: bool },
    /// The string contains interpolation and belongs to a later encapsed mode.
    Interpolated,
}

/// Scans single-quoted strings and non-interpolated double-quoted strings.
pub(crate) fn scan_constant_encapsed_string(source: &str, start: usize) -> Option<StringScan> {
    let bytes = source.as_bytes();
    match bytes.get(start) {
        Some(b'\'') => Some(scan_single_quoted(bytes, start)),
        Some(b'"') => Some(scan_double_quoted(bytes, start)),
        _ => None,
    }
}

fn scan_single_quoted(bytes: &[u8], start: usize) -> StringScan {
    let mut offset = start + 1;
    while offset < bytes.len() {
        match bytes[offset] {
            b'\\' if matches!(bytes.get(offset + 1), Some(b'\\' | b'\'')) => {
                offset += 2;
            }
            b'\'' => {
                return StringScan::Constant {
                    len: offset + 1 - start,
                    terminated: true,
                };
            }
            _ => offset += 1,
        }
    }

    StringScan::Constant {
        len: bytes.len() - start,
        terminated: false,
    }
}

fn scan_double_quoted(bytes: &[u8], start: usize) -> StringScan {
    let mut offset = start + 1;
    while offset < bytes.len() {
        match bytes[offset] {
            b'\\' => {
                offset += 1;
                if offset < bytes.len() {
                    offset += 1;
                }
            }
            b'"' => {
                return StringScan::Constant {
                    len: offset + 1 - start,
                    terminated: true,
                };
            }
            b'$' if starts_interpolation_after_dollar(bytes, offset + 1) => {
                return StringScan::Interpolated;
            }
            b'{' if bytes.get(offset + 1) == Some(&b'$') => return StringScan::Interpolated,
            _ => offset += 1,
        }
    }

    StringScan::Constant {
        len: bytes.len() - start,
        terminated: false,
    }
}

fn starts_interpolation_after_dollar(bytes: &[u8], offset: usize) -> bool {
    matches!(bytes.get(offset), Some(b'{' | b'_'))
        || bytes
            .get(offset)
            .is_some_and(|byte| byte.is_ascii_alphabetic() || *byte >= 0x80)
}

#[cfg(test)]
mod tests {
    use super::{StringScan, scan_constant_encapsed_string};

    #[test]
    fn single_quoted_strings_handle_php_escapes() {
        assert_eq!(
            scan_constant_encapsed_string("'it\\'s'", 0),
            Some(StringScan::Constant {
                len: 7,
                terminated: true
            })
        );
        assert_eq!(
            scan_constant_encapsed_string("'\\\\'", 0),
            Some(StringScan::Constant {
                len: 4,
                terminated: true
            })
        );
    }

    #[test]
    fn double_quoted_strings_without_interpolation_are_constant() {
        assert_eq!(
            scan_constant_encapsed_string("\"\\\\n\"", 0),
            Some(StringScan::Constant {
                len: 5,
                terminated: true
            })
        );
    }

    #[test]
    fn interpolated_double_quoted_strings_are_deferred() {
        assert_eq!(
            scan_constant_encapsed_string("\"$x\"", 0),
            Some(StringScan::Interpolated)
        );
        assert_eq!(
            scan_constant_encapsed_string("\"{$x}\"", 0),
            Some(StringScan::Interpolated)
        );
    }

    #[test]
    fn unterminated_strings_report_length_to_eof() {
        assert_eq!(
            scan_constant_encapsed_string("'abc", 0),
            Some(StringScan::Constant {
                len: 4,
                terminated: false
            })
        );
    }
}
