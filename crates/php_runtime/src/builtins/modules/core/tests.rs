use super::super::debug_output::{php_float_debug_string, php_float_export_string};
use super::{
    BuiltinCompatibility, BuiltinContext, JSON_ERROR_SYNTAX, JSON_PRESERVE_ZERO_FRACTION,
    JSON_UNESCAPED_SLASHES, JSON_UNESCAPED_UNICODE, PHP_QUERY_RFC3986, PHP_RAND_MAX,
    RuntimeSourceSpan, SORT_FLAG_CASE, SORT_NUMERIC, SORT_REGULAR, SORT_STRING,
};
use crate::api::*;
use crate::builtins::context::{
    JSON_BIGINT_AS_STRING, JSON_ERROR_CTRL_CHAR, JSON_ERROR_DEPTH, JSON_ERROR_NON_BACKED_ENUM,
    JSON_ERROR_NONE, JSON_ERROR_STATE_MISMATCH, JSON_FORCE_OBJECT, JSON_HEX_AMP, JSON_HEX_APOS,
    JSON_HEX_QUOT, JSON_HEX_TAG, JSON_NUMERIC_CHECK, JSON_OBJECT_AS_ARRAY,
    JSON_PARTIAL_OUTPUT_ON_ERROR, JSON_PRETTY_PRINT, JSON_THROW_ON_ERROR,
};
use crate::{datetime, layout_stats, pcre};
use std::path::PathBuf;

fn call(name: &str, args: Vec<Value>, output: &mut OutputBuffer) -> Value {
    let entry = BuiltinRegistry::new().get(name).expect("builtin exists");
    let mut context = BuiltinContext::new(output);
    (entry.function())(&mut context, args, RuntimeSourceSpan::default()).expect("builtin ok")
}

fn call_error(name: &str, args: Vec<Value>, output: &mut OutputBuffer) -> String {
    let entry = BuiltinRegistry::new().get(name).expect("builtin exists");
    let mut context = BuiltinContext::new(output);
    (entry.function())(&mut context, args, RuntimeSourceSpan::default())
        .expect_err("builtin should fail")
        .message()
        .to_owned()
}

#[test]
fn recursive_count_stops_at_reference_back_edge() {
    let reference = ReferenceCell::new(Value::Array(PhpArray::new()));
    let mut array = PhpArray::new();
    array.append(Value::Reference(reference.clone()));
    reference.set(Value::Array(array));

    assert_eq!(
        super::baseline_count_recursive_value(&Value::Reference(reference)),
        Some((1, 1))
    );
}

#[test]
fn array_builtin_materialization_copies_scalars_without_clones() {
    layout_stats::reset_layout_stats();
    let materialized = super::materialize_array_builtin_value(&Value::Int(12));
    let stats = layout_stats::take_layout_stats();
    let source_stats = layout_stats::take_layout_source_stats();

    assert_eq!(materialized, Value::Int(12));
    assert_eq!(stats.value_clones, 0, "{stats:?}");
    assert!(
        source_stats.value_clone_by_family.is_empty(),
        "{source_stats:?}"
    );
}

#[test]
fn array_builtin_materialization_attributes_non_scalar_clones() {
    let nested = Value::Array(PhpArray::from_packed(vec![Value::Int(1)]));

    layout_stats::reset_layout_stats();
    layout_stats::enable_layout_source_attribution();
    let materialized = super::materialize_array_builtin_value(&nested);
    let stats = layout_stats::take_layout_stats();
    let source_stats = layout_stats::take_layout_source_stats();

    assert_eq!(materialized, nested);
    assert_eq!(stats.value_clones, 1, "{stats:?}");
    assert_eq!(stats.array_handle_clones, 1, "{stats:?}");
    assert_eq!(
        source_stats
            .value_clone_by_family
            .get(layout_stats::SOURCE_ARRAY_BUILTIN_OUTPUT_MATERIALIZATION.name()),
        Some(&1),
        "{source_stats:?}"
    );
    assert_eq!(
        source_stats
            .array_handle_clone_by_family
            .get(layout_stats::SOURCE_ARRAY_BUILTIN_OUTPUT_MATERIALIZATION.name()),
        Some(&1),
        "{source_stats:?}"
    );
}

#[test]
fn gc_builtins_report_deterministic_noop_state() {
    let mut output = OutputBuffer::new();

    assert_eq!(
        call("gc_collect_cycles", vec![], &mut output),
        Value::Int(0)
    );
    assert_eq!(call("gc_enabled", vec![], &mut output), Value::Bool(true));
    // Stateless test helper calls create independent request contexts. The
    // shared-state behavior is covered through one borrowed request below.
    assert_eq!(call("gc_mem_caches", vec![], &mut output), Value::Int(0));

    let status = call("gc_status", vec![], &mut output);
    let Value::Array(status) = status else {
        panic!("gc_status should return an array");
    };
    assert_eq!(
        status.get(&ArrayKey::String(PhpString::from("running"))),
        Some(&Value::Bool(false))
    );
    assert_eq!(
        status.get(&ArrayKey::String(PhpString::from("collected"))),
        Some(&Value::Int(0))
    );
    assert_eq!(
        status.get(&ArrayKey::String(PhpString::from("threshold"))),
        Some(&Value::Int(10001))
    );
}

#[test]
fn gc_enable_state_is_shared_within_one_request() {
    let registry = BuiltinRegistry::new();
    let mut output = OutputBuffer::new();
    let mut request = BuiltinRequestState::new();
    let mut context = BuiltinContext::new_with_request_state(&mut output, &mut request);
    let mut call = |name: &str| {
        let entry = registry.get(name).expect("builtin exists");
        (entry.function())(&mut context, vec![], RuntimeSourceSpan::default())
            .expect("builtin succeeds")
    };
    assert_eq!(call("gc_enabled"), Value::Bool(true));
    assert_eq!(call("gc_disable"), Value::Null);
    assert_eq!(call("gc_enabled"), Value::Bool(false));
    assert_eq!(call("gc_enable"), Value::Null);
    assert_eq!(call("gc_enabled"), Value::Bool(true));
}

#[test]
fn memory_queries_reject_non_scalar_real_usage_flags() {
    let mut output = OutputBuffer::new();
    for name in ["memory_get_usage", "memory_get_peak_usage"] {
        assert_eq!(
            call_error(name, vec![Value::Array(PhpArray::new())], &mut output),
            format!("{name}(): Argument #1 ($real_usage) must be of type bool, array given")
        );
    }
}

#[test]
fn variable_type_aliases_and_numeric_strings_match_php() {
    let mut output = OutputBuffer::new();
    assert_eq!(
        call("is_integer", vec![Value::Int(1)], &mut output),
        Value::Bool(true)
    );
    assert_eq!(
        call("is_long", vec![Value::Int(1)], &mut output),
        Value::Bool(true)
    );
    assert_eq!(
        call("is_double", vec![Value::float(1.5)], &mut output),
        Value::Bool(true)
    );
    assert_eq!(
        call("is_numeric", vec![Value::string("  1.5e2 ")], &mut output),
        Value::Bool(true)
    );
    assert_eq!(
        call("is_numeric", vec![Value::string("1.5x")], &mut output),
        Value::Bool(false)
    );
    assert_eq!(
        call("is_numeric", vec![Value::Bool(true)], &mut output),
        Value::Bool(false)
    );
}

#[test]
fn mail_accepts_common_sender_argument_shapes() {
    let mut output = OutputBuffer::new();
    assert_eq!(
        call(
            "mail",
            vec![
                Value::string("admin@example.test"),
                Value::string("Subject"),
                Value::string("Body"),
                Value::string("Header: value"),
                Value::string("-fsender@example.test"),
            ],
            &mut output
        ),
        Value::Bool(true)
    );
    assert_eq!(
        call(
            "mail",
            vec![
                Value::string("admin@example.test"),
                Value::string("Subject"),
                Value::string("Body"),
                Value::packed_array(vec![Value::string("Header: value")]),
            ],
            &mut output
        ),
        Value::Bool(true)
    );
}

#[test]
fn gethostbyname_returns_original_host_for_overlong_names() {
    let mut output = OutputBuffer::new();
    let hostname = "a".repeat(256);
    assert_eq!(
        call(
            "gethostbyname",
            vec![Value::string(hostname.clone())],
            &mut output
        ),
        Value::string(hostname)
    );
}

#[test]
fn legacy_random_builtins_return_bounded_ints() {
    let mut output = OutputBuffer::new();
    assert_eq!(
        call("getrandmax", vec![], &mut output),
        Value::Int(i64::from(PHP_RAND_MAX))
    );
    assert_eq!(
        call("mt_getrandmax", vec![], &mut output),
        Value::Int(i64::from(PHP_RAND_MAX))
    );
    for name in ["rand", "mt_rand"] {
        for _ in 0..8 {
            let Value::Int(value) = call(name, vec![Value::Int(3), Value::Int(5)], &mut output)
            else {
                panic!("{name} should return an int");
            };
            assert!(
                (3..=5).contains(&value),
                "{name} returned value outside requested range: {value}"
            );
        }
        let Value::Int(value) = call(name, vec![], &mut output) else {
            panic!("{name} without args should return an int");
        };
        assert!((0..=i64::from(PHP_RAND_MAX)).contains(&value));
        assert_eq!(
            call_error(name, vec![Value::Int(2), Value::Int(1)], &mut output),
            format!("builtin {name}: max must be greater than or equal to min")
        );
    }
}

#[test]
fn checkdate_matches_gregorian_bounds() {
    let mut output = OutputBuffer::new();
    assert_eq!(
        call(
            "checkdate",
            vec![Value::Int(2), Value::Int(29), Value::Int(2006)],
            &mut output
        ),
        Value::Bool(false)
    );
    assert_eq!(
        call(
            "checkdate",
            vec![Value::Int(2), Value::Int(29), Value::Int(2000)],
            &mut output
        ),
        Value::Bool(true)
    );
    assert_eq!(
        call(
            "checkdate",
            vec![Value::Int(1), Value::Int(1), Value::Int(32768)],
            &mut output
        ),
        Value::Bool(false)
    );
}

#[test]
fn variable_debug_float_helpers_match_php_shapes() {
    assert_eq!(php_float_debug_string(1e-5_f64.into(), -1), "1.0E-5");
    assert_eq!(php_float_debug_string((-1e-5_f64).into(), -1), "-1.0E-5");
    assert_eq!(
        php_float_export_string((-0.1_f64).into(), 17),
        "-0.10000000000000001"
    );
    assert_eq!(
        php_float_export_string(1e-5_f64.into(), 17),
        "1.0000000000000001E-5"
    );
    assert_eq!(php_float_export_string(100000.0_f64.into(), 17), "100000.0");
}

#[test]
fn quotemeta_escapes_regex_metacharacters() {
    let mut output = OutputBuffer::new();
    assert_eq!(
        call("quotemeta", vec![Value::string("1+1=2")], &mut output),
        Value::string("1\\+1=2")
    );
    assert_eq!(
        call(
            "quotemeta",
            vec![Value::string("a.b\\c+d*e?f[g^h]i$j(k)l")],
            &mut output,
        ),
        Value::string("a\\.b\\\\c\\+d\\*e\\?f\\[g\\^h\\]i\\$j\\(k\\)l")
    );
    assert_eq!(
        call("quotemeta", vec![Value::string("")], &mut output),
        Value::string("")
    );
    assert_eq!(
        call("quotemeta", vec![Value::string("no specials")], &mut output),
        Value::string("no specials")
    );
}

#[test]
fn sprintf_renders_non_finite_floats_without_padding() {
    let mut output = OutputBuffer::new();
    assert_eq!(
        call(
            "sprintf",
            vec![
                Value::string("%f|%e|%g"),
                Value::float(f64::INFINITY),
                Value::float(f64::INFINITY),
                Value::float(f64::INFINITY),
            ],
            &mut output,
        ),
        Value::string("INF|INF|INF")
    );
    assert_eq!(
        call(
            "sprintf",
            vec![Value::string("%.17g"), Value::float(f64::NEG_INFINITY)],
            &mut output,
        ),
        Value::string("-INF")
    );
    assert_eq!(
        call(
            "sprintf",
            vec![Value::string("%f"), Value::float(f64::NAN)],
            &mut output,
        ),
        Value::string("NaN")
    );
    // PHP ignores width, zero-fill, and the `+` flag for non-finite floats.
    assert_eq!(
        call(
            "sprintf",
            vec![
                Value::string("[%08.2f][%+f]"),
                Value::float(f64::INFINITY),
                Value::float(f64::INFINITY),
            ],
            &mut output,
        ),
        Value::string("[INF][INF]")
    );
}

#[test]
fn native_printf_formatting_visits_direct_output_without_a_result_vector() {
    let arguments = [
        NativePrintfScalar::String(b"item"),
        NativePrintfScalar::Int(7),
    ];
    let mut output = Vec::new();
    assert_eq!(
        visit_native_printf_scalars(
            "sprintf",
            b"%s:%04d",
            arguments.len(),
            |index| arguments.get(index).cloned(),
            |bytes| {
                output.extend_from_slice(bytes);
                Some(())
            },
        ),
        Some(9)
    );
    assert_eq!(output, b"item:0007");
    assert_eq!(
        visit_native_printf_scalars("sprintf", b"%", 0, |_| None, |_| Some(())),
        None
    );

    let mut output = Vec::new();
    assert_eq!(
        visit_native_printf_scalars(
            "sprintf",
            b"%d",
            1,
            |_| Some(NativePrintfScalar::Float(f64::INFINITY)),
            |bytes| {
                output.extend_from_slice(bytes);
                Some(())
            },
        ),
        Some(1)
    );
    assert_eq!(output, b"0");
}

#[test]
fn sprintf_integer_specifiers_use_php_float_casts() {
    let mut output = OutputBuffer::new();
    assert_eq!(
        call(
            "sprintf",
            vec![
                Value::string("%d|%d|%d|%d"),
                Value::float(f64::INFINITY),
                Value::float(f64::NEG_INFINITY),
                Value::float(f64::NAN),
                Value::float(1.0e30),
            ],
            &mut output,
        ),
        Value::string("0|0|0|5076964154930102272")
    );
}

fn call_with_fs(
    name: &str,
    args: Vec<Value>,
    output: &mut OutputBuffer,
    cwd: PathBuf,
    filesystem: FilesystemCapabilities,
) -> Value {
    let entry = BuiltinRegistry::new().get(name).expect("builtin exists");
    let mut context = BuiltinContext::with_runtime(output, cwd, filesystem, None);
    (entry.function())(&mut context, args, RuntimeSourceSpan::default()).expect("builtin ok")
}

fn call_with_fs_resources(
    name: &str,
    args: Vec<Value>,
    output: &mut OutputBuffer,
    cwd: PathBuf,
    filesystem: FilesystemCapabilities,
    resources: &mut ResourceTable,
) -> Value {
    let entry = BuiltinRegistry::new().get(name).expect("builtin exists");
    let mut context = BuiltinContext::with_runtime(output, cwd, filesystem, Some(resources));
    (entry.function())(&mut context, args, RuntimeSourceSpan::default()).expect("builtin ok")
}

fn call_in_context(context: &mut BuiltinContext<'_>, name: &str, args: Vec<Value>) -> Value {
    let entry = BuiltinRegistry::new().get(name).expect("builtin exists");
    (entry.function())(context, args, RuntimeSourceSpan::default()).expect("builtin ok")
}

fn call_with_http_response(
    name: &str,
    args: Vec<Value>,
    response: &mut RuntimeHttpResponseState,
) -> Value {
    let entry = BuiltinRegistry::new().get(name).expect("builtin exists");
    let mut output = OutputBuffer::new();
    let mut context = BuiltinContext::new(&mut output);
    context.set_http_response_state(response);
    (entry.function())(&mut context, args, RuntimeSourceSpan::default()).expect("builtin ok")
}

fn array_strings(value: Value) -> Vec<String> {
    let Value::Array(array) = value else {
        panic!("expected array");
    };
    array
        .iter()
        .map(|(_, value)| match value {
            Value::String(text) => text.to_string_lossy(),
            other => panic!("expected string entry, got {other:?}"),
        })
        .collect()
}

fn array_value(entries: &[(&str, Value)]) -> Value {
    let mut array = PhpArray::new();
    for (key, value) in entries {
        array.insert(
            ArrayKey::String(PhpString::from_test_str(key)),
            value.clone(),
        );
    }
    Value::Array(array)
}

#[test]
fn setcookie_emits_encoded_set_cookie_header() {
    let mut response = RuntimeHttpResponseState::default();

    assert_eq!(
        call_with_http_response(
            "setcookie",
            vec![
                Value::string("login"),
                Value::string("hello world"),
                Value::Int(0),
                Value::string("/"),
                Value::string("example.test"),
                Value::Bool(true),
                Value::Bool(true),
            ],
            &mut response,
        ),
        Value::Bool(true)
    );

    assert_eq!(
        response.headers_list(),
        vec!["Set-Cookie: login=hello%20world; Path=/; Domain=example.test; Secure; HttpOnly"]
    );
}

#[test]
fn setrawcookie_preserves_safe_raw_value() {
    let mut response = RuntimeHttpResponseState::default();

    assert_eq!(
        call_with_http_response(
            "setrawcookie",
            vec![
                Value::string("raw"),
                Value::string("a=b"),
                Value::Int(0),
                Value::string("/raw"),
            ],
            &mut response,
        ),
        Value::Bool(true)
    );

    assert_eq!(
        response.headers_list(),
        vec!["Set-Cookie: raw=a=b; Path=/raw"]
    );
}

#[test]
fn setcookie_options_array_supports_expires_and_samesite() {
    let mut response = RuntimeHttpResponseState::default();

    assert_eq!(
        call_with_http_response(
            "setcookie",
            vec![
                Value::string("prefs"),
                Value::string("dark"),
                array_value(&[
                    ("expires", Value::Int(1_609_459_200)),
                    ("path", Value::string("/app")),
                    ("secure", Value::Bool(true)),
                    ("httponly", Value::Bool(true)),
                    ("samesite", Value::string("Strict")),
                ]),
            ],
            &mut response,
        ),
        Value::Bool(true)
    );

    assert_eq!(
        response.headers_list(),
        vec![
            "Set-Cookie: prefs=dark; Expires=Fri, 01 Jan 2021 00:00:00 GMT; Path=/app; Secure; HttpOnly; SameSite=Strict"
        ]
    );
}

#[test]
fn setcookie_rejects_response_splitting_and_invalid_names() {
    let mut response = RuntimeHttpResponseState::default();

    assert_eq!(
        call_with_http_response(
            "setcookie",
            vec![Value::string("bad\r\nname"), Value::string("ok")],
            &mut response,
        ),
        Value::Bool(false)
    );
    assert_eq!(
        call_with_http_response(
            "setcookie",
            vec![Value::string("good"), Value::string("bad\r\nvalue")],
            &mut response,
        ),
        Value::Bool(false)
    );

    assert!(response.headers.is_empty());
}

#[test]
fn http_response_builtins_track_headers_status_and_cookies() {
    let mut output = OutputBuffer::new();
    let mut context = BuiltinContext::new(&mut output);
    let mut response = RuntimeHttpResponseState::default();
    context.set_http_response_state(&mut response);

    assert_eq!(
        call_in_context(&mut context, "header", vec![Value::string("X-Test: one")]),
        Value::Null
    );
    assert_eq!(
        call_in_context(
            &mut context,
            "header",
            vec![Value::string("X-Test: two"), Value::Bool(false)]
        ),
        Value::Null
    );
    assert_eq!(
        call_in_context(&mut context, "http_response_code", vec![Value::Int(201)]),
        Value::Int(200)
    );
    assert_eq!(
        call_in_context(
            &mut context,
            "setcookie",
            vec![
                Value::string("sid"),
                Value::string("a b"),
                Value::Int(1),
                Value::string("/")
            ],
        ),
        Value::Bool(true)
    );
    assert_eq!(
        call_in_context(
            &mut context,
            "setrawcookie",
            vec![Value::string("raw"), Value::string("a/b")]
        ),
        Value::Bool(true)
    );

    let headers = array_strings(call_in_context(&mut context, "headers_list", Vec::new()));
    assert_eq!(
        headers,
        vec![
            "X-Test: one",
            "X-Test: two",
            "Set-Cookie: sid=a%20b; Expires=Thu, 01 Jan 1970 00:00:01 GMT; Path=/",
            "Set-Cookie: raw=a/b",
        ]
    );
    assert_eq!(
        call_in_context(&mut context, "http_response_code", Vec::new()),
        Value::Int(201)
    );
}

#[test]
fn setcookie_supports_array_options_and_rejects_invalid_names() {
    let mut output = OutputBuffer::new();
    let mut context = BuiltinContext::new(&mut output);
    let mut response = RuntimeHttpResponseState::default();
    context.set_http_response_state(&mut response);
    let mut options = PhpArray::new();
    options.insert(
        ArrayKey::String(PhpString::from_test_str("path")),
        Value::string("/admin"),
    );
    options.insert(
        ArrayKey::String(PhpString::from_test_str("secure")),
        Value::Bool(true),
    );
    options.insert(
        ArrayKey::String(PhpString::from_test_str("httponly")),
        Value::Bool(true),
    );
    options.insert(
        ArrayKey::String(PhpString::from_test_str("samesite")),
        Value::string("Lax"),
    );

    assert_eq!(
        call_in_context(
            &mut context,
            "setcookie",
            vec![
                Value::string("prefs"),
                Value::string("x"),
                Value::Array(options)
            ],
        ),
        Value::Bool(true)
    );
    assert_eq!(
        call_in_context(
            &mut context,
            "setcookie",
            vec![Value::string("bad name"), Value::string("x")],
        ),
        Value::Bool(false)
    );

    let headers = array_strings(call_in_context(&mut context, "headers_list", Vec::new()));
    assert_eq!(
        headers,
        vec!["Set-Cookie: prefs=x; Path=/admin; Secure; HttpOnly; SameSite=Lax"]
    );
    assert!(output.to_string_lossy().contains("invalid cookie name"));
}

#[test]
fn builtins_registry_is_sorted_and_classified() {
    let registry = BuiltinRegistry::new();
    let names = registry
        .entries()
        .iter()
        .map(|entry| entry.name())
        .collect::<Vec<_>>();
    let mut sorted = names.clone();
    sorted.sort_unstable();

    assert_eq!(names, sorted);
    assert!(registry.contains("print"));
    assert!(registry.contains("strlen"));
    assert!(
        registry
            .entries()
            .iter()
            .all(|entry| entry.compatibility() == BuiltinCompatibility::Php)
    );
}

#[test]
fn tokenizer_builtins_use_lexer_lexer_names_and_lines() {
    let mut output = OutputBuffer::new();
    let tokens = call(
        "token_get_all",
        vec![Value::string("<?php echo $name + 1;")],
        &mut output,
    );
    let Value::Array(tokens) = tokens else {
        panic!("expected token array");
    };
    let first = tokens.get(&ArrayKey::Int(0)).expect("open tag token");
    let Value::Array(first) = first else {
        panic!("expected named token entry");
    };
    let id = first.get(&ArrayKey::Int(0)).expect("token id").clone();
    assert_eq!(
        call("token_name", vec![id], &mut output),
        Value::string("T_OPEN_TAG")
    );
    assert_eq!(first.get(&ArrayKey::Int(1)), Some(&Value::string("<?php ")));
    assert_eq!(first.get(&ArrayKey::Int(2)), Some(&Value::Int(1)));

    let names = tokens
        .iter()
        .filter_map(|(_, value)| match value {
            Value::Array(entry) => entry.get(&ArrayKey::Int(0)).cloned(),
            _ => None,
        })
        .map(|id| call("token_name", vec![id], &mut output))
        .collect::<Vec<_>>();
    assert!(names.contains(&Value::string("T_ECHO")));
    assert!(names.contains(&Value::string("T_VARIABLE")));
    assert!(names.contains(&Value::string("T_LNUMBER")));
    assert!(
        tokens
            .iter()
            .any(|(_, value)| matches!(value, Value::String(text) if text.as_bytes() == b"+"))
    );
}

#[test]
fn tokenizer_builtins_return_bad_character_tokens() {
    let mut output = OutputBuffer::new();
    let tokens = call(
        "token_get_all",
        vec![Value::string("<?php \u{0001} foo")],
        &mut output,
    );
    let Value::Array(tokens) = tokens else {
        panic!("expected token array");
    };
    let bad_character = tokens
        .iter()
        .filter_map(|(_, value)| match value {
            Value::Array(entry) => Some(entry),
            _ => None,
        })
        .find(|entry| {
            let Some(id) = entry.get(&ArrayKey::Int(0)).cloned() else {
                return false;
            };
            call("token_name", vec![id], &mut output) == Value::string("T_BAD_CHARACTER")
        })
        .expect("expected T_BAD_CHARACTER entry");
    assert_eq!(
        bad_character.get(&ArrayKey::Int(1)),
        Some(&Value::string("\u{0001}"))
    );
    assert_eq!(bad_character.get(&ArrayKey::Int(2)), Some(&Value::Int(1)));
}

#[test]
fn tokenizer_builtins_cover_modern_php_85_tokens() {
    let mut output = OutputBuffer::new();
    let tokens = call(
        "token_get_all",
        vec![Value::string(
            "<?php class C { public(set) string $name { get => $this->name; } }",
        )],
        &mut output,
    );
    let Value::Array(tokens) = tokens else {
        panic!("expected token array");
    };
    let names = tokens
        .iter()
        .filter_map(|(_, value)| match value {
            Value::Array(entry) => entry.get(&ArrayKey::Int(0)).cloned(),
            _ => None,
        })
        .map(|id| call("token_name", vec![id], &mut output))
        .collect::<Vec<_>>();
    assert!(names.contains(&Value::string("T_PUBLIC_SET")));
    assert!(names.contains(&Value::string("T_VARIABLE")));
    assert_eq!(
        call("token_name", vec![Value::Int(-1)], &mut output),
        Value::string("UNKNOWN")
    );
}

#[test]
fn builtins_cover_scalar_type_queries_and_print() {
    let mut output = OutputBuffer::new();

    assert_eq!(
        call("gettype", vec![Value::Int(7)], &mut output),
        Value::string("integer")
    );
    assert_eq!(
        call("is_int", vec![Value::Int(7)], &mut output),
        Value::Bool(true)
    );
    assert_eq!(
        call("is_string", vec![Value::string("x")], &mut output),
        Value::Bool(true)
    );
    assert_eq!(
        call("is_bool", vec![Value::Bool(false)], &mut output),
        Value::Bool(true)
    );
    assert_eq!(
        call("is_null", vec![Value::Null], &mut output),
        Value::Bool(true)
    );
    assert_eq!(
        call("is_array", vec![Value::packed_array(vec![])], &mut output),
        Value::Bool(true)
    );
    assert_eq!(
        call("is_float", vec![Value::float(1.5)], &mut output),
        Value::Bool(true)
    );
    assert_eq!(
        call("is_scalar", vec![Value::string("x")], &mut output),
        Value::Bool(true)
    );
    assert_eq!(
        call(
            "is_countable",
            vec![Value::packed_array(vec![])],
            &mut output
        ),
        Value::Bool(true)
    );
    assert_eq!(
        call(
            "is_iterable",
            vec![Value::packed_array(vec![])],
            &mut output
        ),
        Value::Bool(true)
    );
    assert_eq!(
        call("print", vec![Value::string("p")], &mut output),
        Value::Int(1)
    );
    assert_eq!(output.to_string_lossy(), "p");
}

#[test]
fn variable_type_builtins_cover_objects_references_and_casts() {
    let mut output = OutputBuffer::new();
    let object = Value::Object(ObjectRef::new_with_display_name(
        &empty_class("DebugBox"),
        "DebugBox",
    ));
    let reference = Value::Reference(ReferenceCell::new(Value::Int(42)));

    assert_eq!(
        call("get_debug_type", vec![object.clone()], &mut output),
        Value::string("DebugBox")
    );
    assert_eq!(
        call("is_object", vec![object], &mut output),
        Value::Bool(true)
    );
    assert_eq!(
        call("gettype", vec![reference.clone()], &mut output),
        Value::string("integer")
    );
    assert_eq!(
        call("is_int", vec![reference], &mut output),
        Value::Bool(true)
    );
    assert_eq!(
        call("boolval", vec![Value::string("0")], &mut output),
        Value::Bool(false)
    );
    assert_eq!(
        call("intval", vec![Value::string("12abc")], &mut output),
        Value::Int(12)
    );
    assert_eq!(
        call(
            "intval",
            vec![Value::string("ff"), Value::Int(16)],
            &mut output
        ),
        Value::Int(255)
    );
    assert_eq!(
        call(
            "intval",
            vec![Value::string("0b1010"), Value::Int(0)],
            &mut output
        ),
        Value::Int(10)
    );
    assert_eq!(
        call(
            "intval",
            vec![Value::string("0b1010"), Value::Int(2)],
            &mut output
        ),
        Value::Int(10)
    );
    assert_eq!(
        call("intval", vec![Value::Int(123), Value::Int(16)], &mut output),
        Value::Int(123)
    );
    assert_eq!(
        call("floatval", vec![Value::string("1.5x")], &mut output),
        Value::float(1.5)
    );
    assert_eq!(
        call("strval", vec![Value::Bool(true)], &mut output),
        Value::string("1")
    );
}

#[test]
fn intval_warns_and_returns_one_for_objects() {
    let mut output = OutputBuffer::new();
    let entry = BuiltinRegistry::new()
        .get("intval")
        .expect("builtin exists");
    let mut context = BuiltinContext::new(&mut output);
    let object = Value::Object(ObjectRef::new_with_display_name(
        &empty_class("NumericObject"),
        "NumericObject",
    ));

    let result = (entry.function())(&mut context, vec![object], RuntimeSourceSpan::default())
        .expect("object conversion continues after its warning");
    let diagnostics = context.take_diagnostics();

    assert_eq!(result, Value::Int(1));
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(
        diagnostics[0].id(),
        "E_PHP_RUNTIME_OBJECT_NUMERIC_CAST_WARNING"
    );
    assert_eq!(
        diagnostics[0].message(),
        "Object of class NumericObject could not be converted to int"
    );
}

#[test]
fn string_cast_builtins_warn_for_array_to_string() {
    let mut output = OutputBuffer::new();

    assert_eq!(
        call(
            "strval",
            vec![Value::packed_array(vec![Value::string("x")])],
            &mut output,
        ),
        Value::string("Array")
    );
    assert_eq!(
        call(
            "sprintf",
            vec![
                Value::string("[%s]"),
                Value::packed_array(vec![Value::string("x")])
            ],
            &mut output,
        ),
        Value::string("[Array]")
    );

    let warnings = output.to_string_lossy();
    assert_eq!(warnings.matches("Array to string conversion").count(), 2);
}

#[test]
fn trim_builtins_support_php_charlists() {
    let mut output = OutputBuffer::new();

    assert_eq!(
        call(
            "trim",
            vec![Value::string(b" \t\r\n\0\x0bABC\0\x0b ".to_vec())],
            &mut output,
        ),
        Value::string("ABC")
    );
    assert_eq!(
        call(
            "trim",
            vec![
                Value::string(b"\n\rExample string\n\r".to_vec()),
                Value::string(b"\x00..\x1f".to_vec()),
            ],
            &mut output,
        ),
        Value::string("Example string")
    );
    assert_eq!(
        call(
            "trim",
            vec![Value::string("  Hello World\n"), Value::string("..a")],
            &mut output,
        ),
        Value::string("  Hello World\n")
    );
    assert!(
        output
            .to_string_lossy()
            .contains("trim(): Invalid '..'-range, no character to the left of '..'")
    );
}

#[test]
fn wordwrap_handles_php_width_edge_cases() {
    let mut output = OutputBuffer::new();

    assert_eq!(
        call(
            "wordwrap",
            vec![
                Value::string("Testing wordrap function"),
                Value::Int(1),
                Value::string(" "),
                Value::Bool(true),
            ],
            &mut output,
        ),
        Value::string("T e s t i n g w o r d r a p f u n c t i o n")
    );
    assert_eq!(
        call(
            "wordwrap",
            vec![
                Value::string("testing wordwrap function"),
                Value::Int(0),
                Value::string("<br />\\n"),
                Value::Bool(false),
            ],
            &mut output,
        ),
        Value::string("testing<br />\\nwordwrap<br />\\nfunction")
    );
    assert_eq!(
        call_error(
            "wordwrap",
            vec![
                Value::string("testing"),
                Value::Int(0),
                Value::string("<br />\\n"),
                Value::Bool(true),
            ],
            &mut output,
        ),
        "wordwrap(): Argument #4 ($cut_long_words) cannot be true when argument #2 ($width) is 0"
    );
    assert_eq!(
        call(
            "wordwrap",
            vec![
                Value::string("123  123ab123"),
                Value::Int(3),
                Value::string("ab")
            ],
            &mut output,
        ),
        Value::string("123ab 123ab123")
    );
    assert_eq!(
        call(
            "wordwrap",
            vec![
                Value::string("123ab123ab123"),
                Value::Int(3),
                Value::string("ab"),
                Value::Bool(true),
            ],
            &mut output,
        ),
        Value::string("123ab123ab123")
    );
    assert_eq!(
        call(
            "wordwrap",
            vec![
                Value::string("123 1234567890 123"),
                Value::Int(10),
                Value::string("|=="),
                Value::Bool(true),
            ],
            &mut output,
        ),
        Value::string("123|==1234567890|==123")
    );
}

#[test]
fn wordwrap_reports_memory_limit_before_huge_break_allocation() {
    let mut output = OutputBuffer::new();
    let error = call_error(
        "wordwrap",
        vec![
            Value::string(vec![b'x'; 65_534]),
            Value::Int(1),
            Value::string(vec![b'x'; 65_535]),
        ],
        &mut output,
    );

    assert_eq!(
        error,
        "Allowed memory size of 134217728 bytes exhausted (tried to allocate 4294705155 bytes)"
    );
    let output = output.to_string_lossy();
    assert!(output.contains("Fatal error: Allowed memory size of 134217728 bytes exhausted"));
    assert!(output.contains("(tried to allocate 4294705155 bytes)"));
}

#[test]
fn resource_type_builtins_report_open_and_closed_handles() {
    let mut output = OutputBuffer::new();
    let mut resources = ResourceTable::new();
    let resource = Value::Resource(resources.register_stream(
        StreamFlags::new(true, true, false),
        StreamMetadata::new("php", "stream", "r+", "php://memory"),
    ));

    assert_eq!(
        call("is_resource", vec![resource.clone()], &mut output),
        Value::Bool(true)
    );
    assert_eq!(
        call("get_resource_id", vec![resource.clone()], &mut output),
        Value::Int(1)
    );
    assert_eq!(
        call("get_resource_type", vec![resource.clone()], &mut output),
        Value::string("stream")
    );
    assert_eq!(
        call("gettype", vec![resource.clone()], &mut output),
        Value::string("resource")
    );
    assert_eq!(
        call("get_debug_type", vec![resource.clone()], &mut output),
        Value::string("resource (stream)")
    );

    assert!(resources.close(ResourceId::new(1)));
    assert!(!resources.close(ResourceId::new(1)));
    assert_eq!(
        call("get_resource_type", vec![resource.clone()], &mut output),
        Value::string("Unknown")
    );
    assert_eq!(
        call_error("get_resource_id", vec![Value::Null], &mut output),
        "get_resource_id(): Argument #1 ($resource) must be of type resource, null given"
    );
    assert_eq!(
        call_error("get_resource_type", vec![Value::Null], &mut output),
        "get_resource_type(): Argument #1 ($resource) must be of type resource, null given"
    );
}

#[test]
fn get_resources_returns_id_keyed_snapshot_and_filters_by_type() {
    let mut output = OutputBuffer::new();
    let mut resources = ResourceTable::new();
    let first = resources.register_stream(
        StreamFlags::new(true, false, false),
        StreamMetadata::new("php", "stream", "r", "php://memory"),
    );
    let context_resource = resources.register_stream_context(PhpArray::new());
    let second = resources.register_stream(
        StreamFlags::new(true, true, false),
        StreamMetadata::new("php", "stream", "r+", "php://temp"),
    );
    assert!(resources.close(first.id()));

    assert_resource_array(
        call_with_fs_resources(
            "get_resources",
            vec![],
            &mut output,
            PathBuf::from("/tmp"),
            FilesystemCapabilities::none(),
            &mut resources,
        ),
        &[
            (1, first.clone()),
            (2, context_resource.clone()),
            (3, second.clone()),
        ],
    );
    assert_resource_array(
        call_with_fs_resources(
            "get_resources",
            vec![Value::string("Unknown")],
            &mut output,
            PathBuf::from("/tmp"),
            FilesystemCapabilities::none(),
            &mut resources,
        ),
        &[(1, first.clone())],
    );
    assert_resource_array(
        call_with_fs_resources(
            "get_resources",
            vec![Value::string("stream")],
            &mut output,
            PathBuf::from("/tmp"),
            FilesystemCapabilities::none(),
            &mut resources,
        ),
        &[(3, second.clone())],
    );
    assert_resource_array(
        call_with_fs_resources(
            "get_resources",
            vec![Value::string("stream-context")],
            &mut output,
            PathBuf::from("/tmp"),
            FilesystemCapabilities::none(),
            &mut resources,
        ),
        &[(2, context_resource.clone())],
    );

    let entry = BuiltinRegistry::new()
        .get("get_resources")
        .expect("builtin exists");
    let mut context = BuiltinContext::with_runtime(
        &mut output,
        PathBuf::from("/tmp"),
        FilesystemCapabilities::none(),
        Some(&mut resources),
    );
    let error = (entry.function())(
        &mut context,
        vec![Value::string("not-a-type")],
        RuntimeSourceSpan::default(),
    )
    .expect_err("invalid resource type should fail");
    assert_eq!(
        error.message(),
        "get_resources(): Argument #1 ($type) must be a valid resource type"
    );
}

fn assert_resource_array(value: Value, expected: &[(i64, crate::ResourceRef)]) {
    let Value::Array(array) = value else {
        panic!("expected array");
    };
    let actual = array.iter().collect::<Vec<_>>();
    assert_eq!(actual.len(), expected.len());
    for ((key, value), (expected_key, expected_resource)) in actual.iter().zip(expected) {
        assert_eq!(*key, ArrayKey::Int(*expected_key));
        assert_eq!(**value, Value::Resource(expected_resource.clone()));
    }
}

#[test]
fn path_helpers_cover_basename_dirname_and_pathinfo() {
    let mut output = OutputBuffer::new();

    assert_eq!(
        call(
            "basename",
            vec![Value::string("/tmp/example.php"), Value::string(".php")],
            &mut output
        ),
        Value::string("example")
    );
    assert_eq!(
        call(
            "basename",
            vec![Value::string("example.php"), Value::string("example.php")],
            &mut output
        ),
        Value::string("example.php")
    );
    assert_eq!(
        call("dirname", vec![Value::string("/tmp/a/b.php")], &mut output),
        Value::string("/tmp/a")
    );
    assert_eq!(
        call("dirname", vec![Value::string("")], &mut output),
        Value::string("")
    );
    let Value::Array(info) = call("pathinfo", vec![Value::string("/tmp/a/b.php")], &mut output)
    else {
        panic!("pathinfo should return array");
    };
    assert_eq!(
        info.get(&ArrayKey::String(PhpString::from_test_str("dirname"))),
        Some(&Value::string("/tmp/a"))
    );
    assert_eq!(
        info.get(&ArrayKey::String(PhpString::from_test_str("basename"))),
        Some(&Value::string("b.php"))
    );
    assert_eq!(
        info.get(&ArrayKey::String(PhpString::from_test_str("extension"))),
        Some(&Value::string("php"))
    );
    assert_eq!(
        info.get(&ArrayKey::String(PhpString::from_test_str("filename"))),
        Some(&Value::string("b"))
    );
    let Value::Array(empty_info) = call("pathinfo", vec![Value::string("")], &mut output) else {
        panic!("pathinfo should return array");
    };
    assert_eq!(
        empty_info.get(&ArrayKey::String(PhpString::from_test_str("dirname"))),
        None
    );
    assert_eq!(
        empty_info.get(&ArrayKey::String(PhpString::from_test_str("basename"))),
        Some(&Value::string(""))
    );
    assert_eq!(
        empty_info.get(&ArrayKey::String(PhpString::from_test_str("filename"))),
        Some(&Value::string(""))
    );

    let Value::Array(dot_info) = call("pathinfo", vec![Value::string(".")], &mut output) else {
        panic!("pathinfo should return array");
    };
    assert_eq!(
        dot_info.get(&ArrayKey::String(PhpString::from_test_str("extension"))),
        Some(&Value::string(""))
    );
    assert_eq!(
        dot_info.get(&ArrayKey::String(PhpString::from_test_str("filename"))),
        Some(&Value::string(""))
    );

    let Value::Array(dotfile_info) =
        call("pathinfo", vec![Value::string(".cvsignore")], &mut output)
    else {
        panic!("pathinfo should return array");
    };
    assert_eq!(
        dotfile_info.get(&ArrayKey::String(PhpString::from_test_str("extension"))),
        Some(&Value::string("cvsignore"))
    );
    assert_eq!(
        dotfile_info.get(&ArrayKey::String(PhpString::from_test_str("filename"))),
        Some(&Value::string(""))
    );

    assert_eq!(
        call(
            "pathinfo",
            vec![
                Value::string("/usr/include/arpa/inet.h"),
                Value::Int(1 | 4 | 8),
            ],
            &mut output
        ),
        Value::string("/usr/include/arpa")
    );
    assert_eq!(
        call(
            "pathinfo",
            vec![
                Value::string("/usr/include/arpa/inet.h"),
                Value::Int(2 | 4 | 8),
            ],
            &mut output
        ),
        Value::string("inet.h")
    );
}

#[test]
fn stat_builtins_are_restricted_to_allowed_roots() {
    let root = std::env::temp_dir().join(format!("phrust-stdlib-stat-{}", std::process::id()));
    std::fs::create_dir_all(&root).expect("create temp root");
    let file = root.join("fixture.txt");
    std::fs::write(&file, b"fixture").expect("write fixture");
    let mut output = OutputBuffer::new();

    assert_eq!(
        call_with_fs(
            "file_exists",
            vec![Value::string(file.to_string_lossy().as_bytes().to_vec())],
            &mut output,
            PathBuf::from("."),
            FilesystemCapabilities::none()
        ),
        Value::Bool(false)
    );

    let capabilities = FilesystemCapabilities::none().with_allowed_roots(vec![root.clone()]);
    assert_eq!(
        call_with_fs(
            "file_exists",
            vec![Value::string("fixture.txt")],
            &mut output,
            root.clone(),
            capabilities.clone()
        ),
        Value::Bool(true)
    );
    assert_eq!(
        call_with_fs(
            "is_file",
            vec![Value::string("fixture.txt")],
            &mut output,
            root.clone(),
            capabilities.clone()
        ),
        Value::Bool(true)
    );
    assert_eq!(
        call_with_fs(
            "is_dir",
            vec![Value::string(".")],
            &mut output,
            root.clone(),
            capabilities.clone()
        ),
        Value::Bool(true)
    );
    assert_eq!(
        call_with_fs(
            "filesize",
            vec![Value::string("fixture.txt")],
            &mut output,
            root.clone(),
            capabilities.clone()
        ),
        Value::Int(7)
    );
    assert_eq!(
        call_with_fs(
            "filetype",
            vec![Value::string("fixture.txt")],
            &mut output,
            root.clone(),
            capabilities.clone()
        ),
        Value::string("file")
    );
    assert!(matches!(
        call_with_fs(
            "stat",
            vec![Value::string("fixture.txt")],
            &mut output,
            root.clone(),
            capabilities.clone()
        ),
        Value::Array(_)
    ));
    assert!(matches!(
        call_with_fs(
            "realpath",
            vec![Value::string("fixture.txt")],
            &mut output,
            root.clone(),
            capabilities
        ),
        Value::String(_)
    ));
    assert_eq!(call("clearstatcache", Vec::new(), &mut output), Value::Null);

    let _ = std::fs::remove_file(file);
    let _ = std::fs::remove_dir(root);
}

#[test]
fn file_get_contents_reads_php_input_from_request_context() {
    let root = std::env::temp_dir().join(format!("phrust-stdlib-input-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("create temp root");
    let capabilities = FilesystemCapabilities::none().with_allowed_roots(vec![root.clone()]);
    let mut output = OutputBuffer::new();
    let mut resources = ResourceTable::new();
    let mut context = BuiltinContext::with_runtime(
        &mut output,
        root.clone(),
        capabilities,
        Some(&mut resources),
    );
    context.set_php_input(b"name=phrust".to_vec());

    assert_eq!(
        call_in_context(
            &mut context,
            "file_get_contents",
            vec![Value::string("php://input")]
        ),
        Value::string("name=phrust")
    );

    let _ = std::fs::remove_dir(root);
}

#[test]
fn file_get_contents_accepts_offset_and_length_arguments() {
    let root = std::env::temp_dir().join(format!(
        "phrust-stdlib-file-get-contents-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("create temp root");
    std::fs::write(root.join("fixture.txt"), b"abcdef").expect("write fixture");
    let capabilities = FilesystemCapabilities::none().with_allowed_roots(vec![root.clone()]);
    let mut output = OutputBuffer::new();

    assert_eq!(
        call_with_fs(
            "file_get_contents",
            vec![
                Value::string("fixture.txt"),
                Value::Bool(false),
                Value::Null,
                Value::Int(0),
                Value::Int(3),
            ],
            &mut output,
            root.clone(),
            capabilities.clone()
        ),
        Value::string("abc")
    );
    assert_eq!(
        call_with_fs(
            "file_get_contents",
            vec![
                Value::string("fixture.txt"),
                Value::Bool(false),
                Value::Null,
                Value::Int(1),
                Value::Int(3),
            ],
            &mut output,
            root.clone(),
            capabilities
        ),
        Value::string("bcd")
    );

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn file_handle_builtins_cover_read_write_seek_and_modes() {
    let root = std::env::temp_dir().join(format!("phrust-stdlib-fileio-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("create temp root");
    let capabilities = FilesystemCapabilities::none().with_allowed_roots(vec![root.clone()]);
    let mut output = OutputBuffer::new();
    let mut resources = ResourceTable::new();

    let handle = call_with_fs_resources(
        "fopen",
        vec![Value::string("data.txt"), Value::string("w+")],
        &mut output,
        root.clone(),
        capabilities.clone(),
        &mut resources,
    );
    assert!(matches!(handle, Value::Resource(_)));
    assert_eq!(
        call_with_fs_resources(
            "fwrite",
            vec![handle.clone(), Value::string("alpha\nbeta")],
            &mut output,
            root.clone(),
            capabilities.clone(),
            &mut resources,
        ),
        Value::Int(10)
    );
    assert_eq!(
        call_with_fs_resources(
            "rewind",
            vec![handle.clone()],
            &mut output,
            root.clone(),
            capabilities.clone(),
            &mut resources,
        ),
        Value::Bool(true)
    );
    assert_eq!(
        call_with_fs_resources(
            "fgets",
            vec![handle.clone()],
            &mut output,
            root.clone(),
            capabilities.clone(),
            &mut resources,
        ),
        Value::string("alpha\n")
    );
    assert_eq!(
        call_with_fs_resources(
            "fgetc",
            vec![handle.clone()],
            &mut output,
            root.clone(),
            capabilities.clone(),
            &mut resources,
        ),
        Value::string("b")
    );
    assert_eq!(
        call_with_fs_resources(
            "fseek",
            vec![handle.clone(), Value::Int(0)],
            &mut output,
            root.clone(),
            capabilities.clone(),
            &mut resources,
        ),
        Value::Int(0)
    );
    assert_eq!(
        call_with_fs_resources(
            "fread",
            vec![handle.clone(), Value::Int(5)],
            &mut output,
            root.clone(),
            capabilities.clone(),
            &mut resources,
        ),
        Value::string("alpha")
    );
    assert_eq!(
        call_with_fs_resources(
            "ftell",
            vec![handle.clone()],
            &mut output,
            root.clone(),
            capabilities.clone(),
            &mut resources,
        ),
        Value::Int(5)
    );
    assert_eq!(
        call_with_fs_resources(
            "fread",
            vec![handle.clone(), Value::Int(99)],
            &mut output,
            root.clone(),
            capabilities.clone(),
            &mut resources,
        ),
        Value::string("\nbeta")
    );
    assert_eq!(
        call_with_fs_resources(
            "feof",
            vec![handle.clone()],
            &mut output,
            root.clone(),
            capabilities.clone(),
            &mut resources,
        ),
        Value::Bool(true)
    );
    assert_eq!(
        call_with_fs_resources(
            "fflush",
            vec![handle.clone()],
            &mut output,
            root.clone(),
            capabilities.clone(),
            &mut resources,
        ),
        Value::Bool(true)
    );
    assert_eq!(
        call_with_fs_resources(
            "fclose",
            vec![handle],
            &mut output,
            root.clone(),
            capabilities.clone(),
            &mut resources,
        ),
        Value::Bool(true)
    );

    let readable = call_with_fs_resources(
        "fopen",
        vec![Value::string("data.txt"), Value::string("r")],
        &mut output,
        root.clone(),
        capabilities.clone(),
        &mut resources,
    );
    assert!(matches!(readable, Value::Resource(_)));
    assert_eq!(
        call_with_fs_resources(
            "fclose",
            vec![readable],
            &mut output,
            root.clone(),
            capabilities.clone(),
            &mut resources,
        ),
        Value::Bool(true)
    );

    assert_eq!(
        call_with_fs(
            "file_put_contents",
            vec![Value::string("append.txt"), Value::string("one")],
            &mut output,
            root.clone(),
            capabilities.clone(),
        ),
        Value::Int(3)
    );
    let append = call_with_fs_resources(
        "fopen",
        vec![Value::string("append.txt"), Value::string("a+")],
        &mut output,
        root.clone(),
        capabilities.clone(),
        &mut resources,
    );
    assert_eq!(
        call_with_fs_resources(
            "fwrite",
            vec![append.clone(), Value::string("two")],
            &mut output,
            root.clone(),
            capabilities.clone(),
            &mut resources,
        ),
        Value::Int(3)
    );
    assert_eq!(
        call_with_fs_resources(
            "fclose",
            vec![append],
            &mut output,
            root.clone(),
            capabilities.clone(),
            &mut resources,
        ),
        Value::Bool(true)
    );
    assert_eq!(
        call_with_fs(
            "file_get_contents",
            vec![Value::string("append.txt")],
            &mut output,
            root.clone(),
            capabilities.clone(),
        ),
        Value::string("onetwo")
    );
    assert_eq!(
        call_with_fs(
            "file_put_contents",
            vec![
                Value::string("append.txt"),
                Value::string("three"),
                Value::Int(10)
            ],
            &mut output,
            root.clone(),
            capabilities.clone(),
        ),
        Value::Int(5)
    );
    assert_eq!(
        call_with_fs(
            "file_get_contents",
            vec![Value::string("append.txt")],
            &mut output,
            root.clone(),
            capabilities.clone(),
        ),
        Value::string("onetwothree")
    );

    assert_eq!(
        call_with_fs_resources(
            "fopen",
            vec![Value::string("append.txt"), Value::string("x")],
            &mut output,
            root.clone(),
            capabilities.clone(),
            &mut resources,
        ),
        Value::Bool(false)
    );
    let exclusive = call_with_fs_resources(
        "fopen",
        vec![Value::string("exclusive.txt"), Value::string("x")],
        &mut output,
        root.clone(),
        capabilities.clone(),
        &mut resources,
    );
    assert!(matches!(exclusive, Value::Resource(_)));
    assert_eq!(
        call_with_fs_resources(
            "fclose",
            vec![exclusive],
            &mut output,
            root.clone(),
            capabilities.clone(),
            &mut resources,
        ),
        Value::Bool(true)
    );

    assert_eq!(
        call_with_fs(
            "file_put_contents",
            vec![Value::string("create.txt"), Value::string("keep")],
            &mut output,
            root.clone(),
            capabilities.clone(),
        ),
        Value::Int(4)
    );
    let create = call_with_fs_resources(
        "fopen",
        vec![Value::string("create.txt"), Value::string("c+")],
        &mut output,
        root.clone(),
        capabilities.clone(),
        &mut resources,
    );
    assert!(matches!(create, Value::Resource(_)));
    assert_eq!(
        call_with_fs_resources(
            "fclose",
            vec![create],
            &mut output,
            root.clone(),
            capabilities.clone(),
            &mut resources,
        ),
        Value::Bool(true)
    );
    assert_eq!(
        call_with_fs(
            "file_get_contents",
            vec![Value::string("create.txt")],
            &mut output,
            root.clone(),
            capabilities,
        ),
        Value::string("keep")
    );

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn file_operations_are_root_constrained_and_return_false() {
    let root = std::env::temp_dir().join(format!("phrust-stdlib-fileops-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("create temp root");
    let capabilities = FilesystemCapabilities::none().with_allowed_roots(vec![root.clone()]);
    let mut output = OutputBuffer::new();
    let mut resources = ResourceTable::new();

    assert_eq!(
        call_with_fs(
            "file_get_contents",
            vec![Value::string(
                root.join("outside.txt")
                    .to_string_lossy()
                    .as_bytes()
                    .to_vec()
            )],
            &mut output,
            PathBuf::from("."),
            FilesystemCapabilities::none(),
        ),
        Value::Bool(false)
    );
    assert_eq!(
        call_with_fs_resources(
            "fopen",
            vec![Value::string("../escape.txt"), Value::string("w")],
            &mut output,
            root.clone(),
            capabilities.clone(),
            &mut resources,
        ),
        Value::Bool(false)
    );

    assert_eq!(
        call_with_fs(
            "file_put_contents",
            vec![Value::string("fixture.txt"), Value::string("hello")],
            &mut output,
            root.clone(),
            capabilities.clone(),
        ),
        Value::Int(5)
    );
    assert_eq!(
        call_with_fs(
            "file_get_contents",
            vec![Value::string("fixture.txt")],
            &mut output,
            root.clone(),
            capabilities.clone(),
        ),
        Value::string("hello")
    );

    let mut read_output = OutputBuffer::new();
    assert_eq!(
        call_with_fs(
            "readfile",
            vec![Value::string("fixture.txt")],
            &mut read_output,
            root.clone(),
            capabilities.clone(),
        ),
        Value::Int(5)
    );
    assert_eq!(read_output.to_string_lossy(), "hello");

    assert_eq!(
        call_with_fs(
            "copy",
            vec![Value::string("fixture.txt"), Value::string("fixture.txt")],
            &mut output,
            root.clone(),
            capabilities.clone(),
        ),
        Value::Bool(false)
    );
    assert_eq!(
        call_with_fs(
            "copy",
            vec![
                Value::string(
                    root.join("fixture.txt")
                        .to_string_lossy()
                        .as_bytes()
                        .to_vec()
                ),
                Value::string("fixture.txt")
            ],
            &mut output,
            root.clone(),
            capabilities.clone(),
        ),
        Value::Bool(false)
    );
    assert_eq!(
        call_with_fs(
            "copy",
            vec![Value::string("fixture.txt"), Value::string("copy.txt")],
            &mut output,
            root.clone(),
            capabilities.clone(),
        ),
        Value::Bool(true)
    );
    assert_eq!(
        call_with_fs(
            "rename",
            vec![Value::string("copy.txt"), Value::string("renamed.txt")],
            &mut output,
            root.clone(),
            capabilities.clone(),
        ),
        Value::Bool(true)
    );
    assert_eq!(
        call_with_fs(
            "touch",
            vec![Value::string("touched.txt")],
            &mut output,
            root.clone(),
            capabilities.clone(),
        ),
        Value::Bool(true)
    );
    assert_eq!(
        call_with_fs(
            "mkdir",
            vec![Value::string("nested")],
            &mut output,
            root.clone(),
            capabilities.clone(),
        ),
        Value::Bool(true)
    );
    assert_eq!(
        call_with_fs(
            "rmdir",
            vec![Value::string("nested")],
            &mut output,
            root.clone(),
            capabilities.clone(),
        ),
        Value::Bool(true)
    );
    assert_eq!(
        call_with_fs(
            "unlink",
            vec![Value::string("renamed.txt")],
            &mut output,
            root.clone(),
            capabilities.clone(),
        ),
        Value::Bool(true)
    );

    let temp_path = call_with_fs(
        "tempnam",
        vec![Value::string("."), Value::string("pre")],
        &mut output,
        root.clone(),
        capabilities.clone(),
    );
    assert!(matches!(temp_path, Value::String(_)));
    let tmp_handle = call_with_fs_resources(
        "tmpfile",
        Vec::new(),
        &mut output,
        root.clone(),
        capabilities.clone(),
        &mut resources,
    );
    assert!(matches!(tmp_handle, Value::Resource(_)));

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn directory_handles_read_rewind_and_close_with_sorted_entries() {
    let root = std::env::temp_dir().join(format!("phrust-stdlib-dir-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("create temp root");
    std::fs::write(root.join("b.log"), b"b").expect("write fixture");
    std::fs::write(root.join("a.txt"), b"a").expect("write fixture");
    let capabilities = FilesystemCapabilities::none().with_allowed_roots(vec![root.clone()]);
    let mut output = OutputBuffer::new();
    let mut resources = ResourceTable::new();

    let handle = call_with_fs_resources(
        "opendir",
        vec![Value::string(".")],
        &mut output,
        root.clone(),
        capabilities.clone(),
        &mut resources,
    );
    assert!(matches!(handle, Value::Resource(_)));
    assert_eq!(
        call_with_fs_resources(
            "readdir",
            vec![handle.clone()],
            &mut output,
            root.clone(),
            capabilities.clone(),
            &mut resources,
        ),
        Value::string(".")
    );
    assert_eq!(
        call_with_fs_resources(
            "readdir",
            vec![handle.clone()],
            &mut output,
            root.clone(),
            capabilities.clone(),
            &mut resources,
        ),
        Value::string("..")
    );
    assert_eq!(
        call_with_fs_resources(
            "readdir",
            vec![handle.clone()],
            &mut output,
            root.clone(),
            capabilities.clone(),
            &mut resources,
        ),
        Value::string("a.txt")
    );
    assert_eq!(
        call_with_fs_resources(
            "rewinddir",
            vec![handle.clone()],
            &mut output,
            root.clone(),
            capabilities.clone(),
            &mut resources,
        ),
        Value::Null
    );
    assert_eq!(
        call_with_fs_resources(
            "readdir",
            vec![handle.clone()],
            &mut output,
            root.clone(),
            capabilities.clone(),
            &mut resources,
        ),
        Value::string(".")
    );
    assert_eq!(
        call_with_fs_resources(
            "closedir",
            vec![handle],
            &mut output,
            root.clone(),
            capabilities,
            &mut resources,
        ),
        Value::Null
    );

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn scandir_glob_and_directory_capabilities_are_normalized() {
    let root = std::env::temp_dir().join(format!("phrust-stdlib-glob-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("nested")).expect("create temp root");
    std::fs::write(root.join("b.log"), b"b").expect("write fixture");
    std::fs::write(root.join("a.txt"), b"a").expect("write fixture");
    let capabilities = FilesystemCapabilities::none().with_allowed_roots(vec![root.clone()]);
    let mut output = OutputBuffer::new();
    let mut resources = ResourceTable::new();

    assert_eq!(
        call_with_fs_resources(
            "opendir",
            vec![Value::string(root.to_string_lossy().as_bytes().to_vec())],
            &mut output,
            PathBuf::from("."),
            FilesystemCapabilities::none(),
            &mut resources,
        ),
        Value::Bool(false)
    );
    assert_eq!(
        array_strings(call_with_fs(
            "scandir",
            vec![Value::string(".")],
            &mut output,
            root.clone(),
            capabilities.clone(),
        )),
        vec![".", "..", "a.txt", "b.log", "nested"]
    );
    assert_eq!(
        array_strings(call_with_fs(
            "scandir",
            vec![Value::string("."), Value::Int(1)],
            &mut output,
            root.clone(),
            capabilities.clone(),
        )),
        vec!["nested", "b.log", "a.txt", "..", "."]
    );
    let globbed = array_strings(call_with_fs(
        "glob",
        vec![Value::string("*.txt")],
        &mut output,
        root.clone(),
        capabilities,
    ));
    assert_eq!(globbed.len(), 1);
    assert!(globbed[0].ends_with("a.txt"));

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn getcwd_and_chdir_are_request_local_to_builtin_context() {
    let root = std::env::temp_dir().join(format!("phrust-stdlib-cwd-{}", std::process::id()));
    let nested = root.join("nested");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&nested).expect("create temp root");
    let capabilities = FilesystemCapabilities::none().with_allowed_roots(vec![root.clone()]);
    let mut output = OutputBuffer::new();
    let mut context = BuiltinContext::with_runtime(&mut output, root.clone(), capabilities, None);

    assert_eq!(
        call_in_context(&mut context, "getcwd", Vec::new()),
        Value::string(root.to_string_lossy().as_bytes().to_vec())
    );
    assert_eq!(
        call_in_context(&mut context, "chdir", vec![Value::string("nested")]),
        Value::Bool(true)
    );
    assert_eq!(
        call_in_context(&mut context, "getcwd", Vec::new()),
        Value::string(nested.to_string_lossy().as_bytes().to_vec())
    );
    assert_eq!(
        call_in_context(&mut context, "chdir", vec![Value::string("../..")]),
        Value::Bool(false)
    );

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn stream_metadata_contents_copy_and_local_checks_are_capability_aware() {
    let root = std::env::temp_dir().join(format!("phrust-stdlib-streams-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("create temp root");
    let capabilities = FilesystemCapabilities::none().with_allowed_roots(vec![root.clone()]);
    let mut output = OutputBuffer::new();
    let mut resources = ResourceTable::new();

    assert_eq!(
        array_strings(call("stream_get_wrappers", Vec::new(), &mut output)),
        vec!["file".to_string(), "php".to_string()]
    );

    let source = call_with_fs_resources(
        "fopen",
        vec![Value::string("php://memory"), Value::string("w+")],
        &mut output,
        root.clone(),
        capabilities.clone(),
        &mut resources,
    );
    let destination = call_with_fs_resources(
        "fopen",
        vec![Value::string("php://memory"), Value::string("w+")],
        &mut output,
        root.clone(),
        capabilities.clone(),
        &mut resources,
    );
    assert_eq!(
        call_with_fs_resources(
            "fwrite",
            vec![source.clone(), Value::string("abcdef")],
            &mut output,
            root.clone(),
            capabilities.clone(),
            &mut resources,
        ),
        Value::Int(6)
    );
    assert_eq!(
        call_with_fs_resources(
            "stream_get_contents",
            vec![source.clone(), Value::Int(3), Value::Int(2)],
            &mut output,
            root.clone(),
            capabilities.clone(),
            &mut resources,
        ),
        Value::string("cde")
    );
    assert_eq!(
        call_with_fs_resources(
            "stream_copy_to_stream",
            vec![
                source.clone(),
                destination.clone(),
                Value::Int(4),
                Value::Int(0)
            ],
            &mut output,
            root.clone(),
            capabilities.clone(),
            &mut resources,
        ),
        Value::Int(4)
    );
    assert_eq!(
        call_with_fs_resources(
            "rewind",
            vec![destination.clone()],
            &mut output,
            root.clone(),
            capabilities.clone(),
            &mut resources,
        ),
        Value::Bool(true)
    );
    assert_eq!(
        call_with_fs_resources(
            "stream_get_contents",
            vec![destination.clone()],
            &mut output,
            root.clone(),
            capabilities.clone(),
            &mut resources,
        ),
        Value::string("abcd")
    );

    let Value::Array(metadata) = call_with_fs_resources(
        "stream_get_meta_data",
        vec![source.clone()],
        &mut output,
        root.clone(),
        capabilities.clone(),
        &mut resources,
    ) else {
        panic!("expected metadata array");
    };
    assert_eq!(
        metadata.get(&ArrayKey::String(PhpString::from_test_str("wrapper_type"))),
        Some(&Value::string("PHP"))
    );
    assert_eq!(
        metadata.get(&ArrayKey::String(PhpString::from_test_str("stream_type"))),
        Some(&Value::string("MEMORY"))
    );

    assert_eq!(
        call_with_fs_resources(
            "stream_is_local",
            vec![Value::string("https://example.test/file.txt")],
            &mut output,
            root.clone(),
            capabilities.clone(),
            &mut resources,
        ),
        Value::Bool(false)
    );
    assert_eq!(
        call_with_fs_resources(
            "stream_is_local",
            vec![Value::string("php://memory")],
            &mut output,
            root.clone(),
            capabilities.clone(),
            &mut resources,
        ),
        Value::Bool(true)
    );
    assert_eq!(
        call_with_fs_resources(
            "stream_isatty",
            vec![source],
            &mut output,
            root.clone(),
            capabilities,
            &mut resources,
        ),
        Value::Bool(false)
    );

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn stream_context_options_and_include_path_resolution_are_preserved() {
    let root = std::env::temp_dir().join(format!(
        "phrust-stdlib-stream-context-{}",
        std::process::id()
    ));
    let lib = root.join("lib");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&lib).expect("create include dir");
    std::fs::write(lib.join("Foo.php"), b"<?php").expect("write include fixture");
    let capabilities = FilesystemCapabilities::none().with_allowed_roots(vec![root.clone()]);
    let mut output = OutputBuffer::new();
    let mut resources = ResourceTable::new();
    let mut context = BuiltinContext::with_runtime(
        &mut output,
        root.clone(),
        capabilities.clone(),
        Some(&mut resources),
    );
    context.set_include_path(vec![PathBuf::from("lib")]);

    let stream_context = call_in_context(&mut context, "stream_context_create", Vec::new());
    assert!(matches!(stream_context, Value::Resource(_)));
    assert_eq!(
        call_in_context(
            &mut context,
            "stream_context_set_option",
            vec![
                stream_context.clone(),
                Value::string("http"),
                Value::string("timeout"),
                Value::Int(5),
            ],
        ),
        Value::Bool(true)
    );
    let Value::Array(options) = call_in_context(
        &mut context,
        "stream_context_get_options",
        vec![stream_context.clone()],
    ) else {
        panic!("expected context options");
    };
    let Some(Value::Array(http_options)) =
        options.get(&ArrayKey::String(PhpString::from_test_str("http")))
    else {
        panic!("expected http options");
    };
    assert_eq!(
        http_options.get(&ArrayKey::String(PhpString::from_test_str("timeout"))),
        Some(&Value::Int(5))
    );

    let resolved = call_in_context(
        &mut context,
        "stream_resolve_include_path",
        vec![Value::string("Foo.php")],
    );
    let Value::String(path) = resolved else {
        panic!("expected resolved include path");
    };
    assert!(path.to_string_lossy().ends_with("lib/Foo.php"));
    assert_eq!(
        call_in_context(
            &mut context,
            "stream_resolve_include_path",
            vec![Value::string("../escape.php")],
        ),
        Value::Bool(false)
    );

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn preg_match_and_match_all_capture_offsets_and_modifiers() {
    let mut output = OutputBuffer::new();
    let mut context = BuiltinContext::new(&mut output);

    let matches = ReferenceCell::new(Value::Null);
    assert_eq!(
        call_in_context(
            &mut context,
            "preg_match",
            vec![
                Value::string(r#"/([a-z]+)-(\d+)/i"#),
                Value::string("ABC-123"),
                Value::Reference(matches.clone()),
                Value::Int(pcre::PREG_OFFSET_CAPTURE),
            ],
        ),
        Value::Int(1)
    );
    let Value::Array(captures) = matches.get() else {
        panic!("expected captures array");
    };
    assert_eq!(
        captures.get(&ArrayKey::Int(0)),
        Some(&Value::packed_array(vec![
            Value::string("ABC-123"),
            Value::Int(0)
        ]))
    );
    assert_eq!(
        captures.get(&ArrayKey::Int(2)),
        Some(&Value::packed_array(vec![
            Value::string("123"),
            Value::Int(4)
        ]))
    );

    let negative_offset_matches = ReferenceCell::new(Value::Null);
    assert_eq!(
        call_in_context(
            &mut context,
            "preg_match_all",
            vec![
                Value::string(r#"/[0-35-9]/"#),
                Value::string(
                    r#"Hello, world! This is a test. This is another test. \[4]. 34534 string."#
                ),
                Value::Reference(negative_offset_matches.clone()),
                Value::Int(pcre::PREG_PATTERN_ORDER | pcre::PREG_OFFSET_CAPTURE),
                Value::Int(-10),
            ],
        ),
        Value::Int(1)
    );
    let Value::Array(pattern_rows) = negative_offset_matches.get() else {
        panic!("expected pattern-order rows");
    };
    let Some(Value::Array(full_matches)) = pattern_rows.get(&ArrayKey::Int(0)) else {
        panic!("expected full-match row");
    };
    assert_eq!(
        full_matches.get(&ArrayKey::Int(0)),
        Some(&Value::packed_array(vec![
            Value::string("3"),
            Value::Int(61)
        ]))
    );

    let invalid_offset_matches = ReferenceCell::new(Value::Null);
    assert_eq!(
        call_in_context(
            &mut context,
            "preg_match",
            vec![
                Value::string(r#"/\d/"#),
                Value::string("abc123"),
                Value::Reference(invalid_offset_matches.clone()),
                Value::Int(pcre::PREG_OFFSET_CAPTURE),
                Value::Int(7),
            ],
        ),
        Value::Bool(false)
    );
    assert_eq!(
        invalid_offset_matches.get(),
        Value::packed_array(Vec::new())
    );
    assert_eq!(
        call_in_context(&mut context, "preg_last_error", Vec::new()),
        Value::Int(pcre::PREG_INTERNAL_ERROR)
    );
    let mut min_offset_error_output = OutputBuffer::new();
    assert_eq!(
        call_error(
            "preg_match",
            vec![
                Value::string(r#"/\d/"#),
                Value::string("abc123"),
                Value::Reference(ReferenceCell::new(Value::Null)),
                Value::Int(0),
                Value::Int(i64::MIN),
            ],
            &mut min_offset_error_output,
        ),
        "preg_match(): Argument #5 ($offset) must be greater than -9223372036854775808"
    );
    let mut min_offset_all_error_output = OutputBuffer::new();
    assert_eq!(
        call_error(
            "preg_match_all",
            vec![
                Value::string(r#"/\d/"#),
                Value::string("abc123"),
                Value::Reference(ReferenceCell::new(Value::Null)),
                Value::Int(0),
                Value::Int(i64::MIN),
            ],
            &mut min_offset_all_error_output,
        ),
        "preg_match_all(): Argument #5 ($offset) must be greater than -9223372036854775808"
    );
    let mut flag_error_output = OutputBuffer::new();
    assert_eq!(
        call_error(
            "preg_match_all",
            vec![
                Value::string("//"),
                Value::string(""),
                Value::Reference(ReferenceCell::new(Value::Null)),
                Value::Int(0xdead),
            ],
            &mut flag_error_output,
        ),
        "preg_match_all(): Argument #4 ($flags) must be a PREG_* constant"
    );

    let utf8_offset_matches = ReferenceCell::new(Value::Null);
    assert_eq!(
        call_in_context(
            &mut context,
            "preg_match",
            vec![
                Value::string(r#"/a/u"#),
                Value::string(vec![0xE3, 0x82, 0xA2]),
                Value::Reference(utf8_offset_matches.clone()),
                Value::Int(0),
                Value::Int(1),
            ],
        ),
        Value::Bool(false)
    );
    assert_eq!(utf8_offset_matches.get(), Value::packed_array(Vec::new()));
    assert_eq!(
        call_in_context(&mut context, "preg_last_error", Vec::new()),
        Value::Int(pcre::PREG_BAD_UTF8_OFFSET_ERROR)
    );
    assert_eq!(
        call_in_context(&mut context, "preg_last_error_msg", Vec::new()),
        Value::string(pcre::preg_error_message(pcre::PREG_BAD_UTF8_OFFSET_ERROR))
    );

    let invalid_utf8_matches = ReferenceCell::new(Value::Null);
    assert_eq!(
        call_in_context(
            &mut context,
            "preg_match",
            vec![
                Value::string(r#"/./u"#),
                Value::string(vec![0xff]),
                Value::Reference(invalid_utf8_matches.clone()),
            ],
        ),
        Value::Bool(false)
    );
    assert_eq!(invalid_utf8_matches.get(), Value::packed_array(Vec::new()));
    assert_eq!(
        call_in_context(&mut context, "preg_last_error", Vec::new()),
        Value::Int(pcre::PREG_BAD_UTF8_ERROR)
    );
    assert_eq!(
        call_in_context(&mut context, "preg_last_error_msg", Vec::new()),
        Value::string("Malformed UTF-8 characters, possibly incorrectly encoded")
    );

    let invalid_prefix = Value::string(vec![b'V', b'A', 0xff, b'L', b'I', b'D']);
    let invalid_prefix_matches = ReferenceCell::new(Value::Null);
    assert_eq!(
        call_in_context(
            &mut context,
            "preg_match",
            vec![
                Value::string(r#"/\b/u"#),
                invalid_prefix.clone(),
                Value::Reference(invalid_prefix_matches.clone()),
                Value::Int(pcre::PREG_OFFSET_CAPTURE),
                Value::Int(4),
            ],
        ),
        Value::Int(1)
    );
    let Value::Array(invalid_prefix_captures) = invalid_prefix_matches.get() else {
        panic!("expected invalid-prefix captures array");
    };
    assert_eq!(
        invalid_prefix_captures.get(&ArrayKey::Int(0)),
        Some(&Value::packed_array(vec![Value::string(""), Value::Int(6)]))
    );
    assert_eq!(
        call_in_context(&mut context, "preg_last_error", Vec::new()),
        Value::Int(pcre::PREG_NO_ERROR)
    );

    let invalid_prefix_all = ReferenceCell::new(Value::Null);
    assert_eq!(
        call_in_context(
            &mut context,
            "preg_match_all",
            vec![
                Value::string(r#"/\b/u"#),
                invalid_prefix.clone(),
                Value::Reference(invalid_prefix_all.clone()),
                Value::Int(pcre::PREG_PATTERN_ORDER | pcre::PREG_OFFSET_CAPTURE),
                Value::Int(4),
            ],
        ),
        Value::Int(1)
    );
    let Value::Array(invalid_prefix_rows) = invalid_prefix_all.get() else {
        panic!("expected invalid-prefix match-all rows");
    };
    let Some(Value::Array(invalid_prefix_full)) = invalid_prefix_rows.get(&ArrayKey::Int(0)) else {
        panic!("expected invalid-prefix full-match row");
    };
    assert_eq!(
        invalid_prefix_full.get(&ArrayKey::Int(0)),
        Some(&Value::packed_array(vec![Value::string(""), Value::Int(6)]))
    );
    assert_eq!(
        call_in_context(
            &mut context,
            "preg_match",
            vec![
                Value::string(r#"/\b/u"#),
                invalid_prefix,
                Value::Reference(ReferenceCell::new(Value::Null)),
                Value::Int(0),
                Value::Int(0),
            ],
        ),
        Value::Bool(false)
    );
    assert_eq!(
        call_in_context(&mut context, "preg_last_error", Vec::new()),
        Value::Int(pcre::PREG_BAD_UTF8_ERROR)
    );

    let all = ReferenceCell::new(Value::Null);
    assert_eq!(
        call_in_context(
            &mut context,
            "preg_match_all",
            vec![
                Value::string(r#"/([a-z]+)=(\d+)/i"#),
                Value::string("A=1 b=22"),
                Value::Reference(all.clone()),
                Value::Int(pcre::PREG_SET_ORDER | pcre::PREG_OFFSET_CAPTURE),
            ],
        ),
        Value::Int(2)
    );
    let Value::Array(rows) = all.get() else {
        panic!("expected match rows");
    };
    assert_eq!(rows.len(), 2);
    let no_matches = ReferenceCell::new(Value::Null);
    assert_eq!(
        call_in_context(
            &mut context,
            "preg_match_all",
            vec![
                Value::string(r#"/%.+?%/"#),
                Value::string("/"),
                Value::Reference(no_matches.clone()),
            ],
        ),
        Value::Int(0)
    );
    let Value::Array(pattern_order) = no_matches.get() else {
        panic!("expected pattern-order no-match array");
    };
    assert_eq!(pattern_order.len(), 1);
    assert_eq!(
        pattern_order.get(&ArrayKey::Int(0)),
        Some(&Value::packed_array(Vec::new()))
    );
    assert_eq!(
        call_in_context(&mut context, "preg_last_error", Vec::new()),
        Value::Int(pcre::PREG_NO_ERROR)
    );

    let duplicate_name_match = ReferenceCell::new(Value::Null);
    assert_eq!(
        call_in_context(
            &mut context,
            "preg_match",
            vec![
                Value::string(r#"/(?J)(?:(?<g>foo)|(?<g>bar))/"#),
                Value::string("foo"),
                Value::Reference(duplicate_name_match.clone()),
            ],
        ),
        Value::Int(1)
    );
    let Value::Array(duplicate_name_match) = duplicate_name_match.get() else {
        panic!("expected duplicate-name match array");
    };
    assert_eq!(
        duplicate_name_match.get(&ArrayKey::String(PhpString::from_test_str("g"))),
        Some(&Value::string("foo"))
    );
    assert_eq!(
        duplicate_name_match.get(&ArrayKey::Int(1)),
        Some(&Value::string("foo"))
    );
    assert_eq!(duplicate_name_match.get(&ArrayKey::Int(2)), None);

    let duplicate_name_set_order = ReferenceCell::new(Value::Null);
    assert_eq!(
        call_in_context(
            &mut context,
            "preg_match_all",
            vec![
                Value::string(r#"/(?J)(?<chr>[ac])(?<num>\d)|(?<chr>[b])/"#),
                Value::string("a1bc3"),
                Value::Reference(duplicate_name_set_order.clone()),
                Value::Int(pcre::PREG_SET_ORDER),
            ],
        ),
        Value::Int(3)
    );
    let Value::Array(rows) = duplicate_name_set_order.get() else {
        panic!("expected set-order duplicate-name rows");
    };
    let Some(Value::Array(second_row)) = rows.get(&ArrayKey::Int(1)) else {
        panic!("expected second duplicate-name row");
    };
    assert_eq!(
        second_row.get(&ArrayKey::String(PhpString::from_test_str("chr"))),
        Some(&Value::string("b"))
    );
    assert_eq!(second_row.get(&ArrayKey::Int(1)), Some(&Value::string("")));
    assert_eq!(second_row.get(&ArrayKey::Int(3)), Some(&Value::string("b")));

    let named_pattern_order = ReferenceCell::new(Value::Null);
    assert_eq!(
        call_in_context(
            &mut context,
            "preg_match_all",
            vec![
                Value::string(r#"/(?<a>4)?(?<b>2)?\d/"#),
                Value::string("123456"),
                Value::Reference(named_pattern_order.clone()),
                Value::Int(pcre::PREG_UNMATCHED_AS_NULL),
            ],
        ),
        Value::Int(4)
    );
    let Value::Array(named_pattern_order) = named_pattern_order.get() else {
        panic!("expected named pattern-order rows");
    };
    assert!(matches!(
        named_pattern_order.get(&ArrayKey::String(PhpString::from_test_str("a"))),
        Some(Value::Array(_))
    ));
    assert!(matches!(
        named_pattern_order.get(&ArrayKey::String(PhpString::from_test_str("b"))),
        Some(Value::Array(_))
    ));
}

#[test]
fn preg_replace_split_grep_quote_callback_and_errors_are_pcre2_backed() {
    let mut output = OutputBuffer::new();
    let mut context = BuiltinContext::new(&mut output);

    let count = ReferenceCell::new(Value::Null);
    assert_eq!(
        call_in_context(
            &mut context,
            "preg_replace",
            vec![
                Value::string(r#"/([a-z]+)=(\d+)/"#),
                Value::string(r#"$1:$2"#),
                Value::string("a=1 b=22"),
                Value::Int(-1),
                Value::Reference(count.clone()),
            ],
        ),
        Value::string("a:1 b:22")
    );
    assert_eq!(count.get(), Value::Int(2));

    assert_eq!(
        call_in_context(
            &mut context,
            "preg_replace",
            vec![
                Value::string(r#"/(ab)(c)(d)(e)(f)(g)(h)(i)(j)(k)/"#),
                Value::string(r#"a${1}2$103"#),
                Value::string("zabcdefghijkl"),
            ],
        ),
        Value::string("zaab2k3l")
    );
    assert_eq!(
        call_in_context(
            &mut context,
            "preg_replace",
            vec![
                Value::string(r#"/(a)(b)/"#),
                Value::string(r#"\1-$1-${1}-$10-${10}-$99-$001-${001}"#),
                Value::string("ab"),
            ],
        ),
        Value::string("a-a-a----ab1-${001}")
    );
    assert_eq!(
        call_in_context(
            &mut context,
            "preg_replace",
            vec![
                Value::string("/[/"),
                Value::string("x"),
                Value::string("subject"),
            ],
        ),
        Value::Null
    );
    assert_eq!(
        call_in_context(&mut context, "preg_last_error", Vec::new()),
        Value::Int(pcre::PREG_INTERNAL_ERROR)
    );
    let mut malformed_class_output = OutputBuffer::new();
    let mut malformed_class_context = BuiltinContext::new(&mut malformed_class_output);
    assert_eq!(
        call_in_context(
            &mut malformed_class_context,
            "preg_replace",
            vec![
                Value::string(r#"/.++\d*+[/"#),
                Value::string("for ($"),
                Value::string("abc"),
            ],
        ),
        Value::Null
    );
    assert!(String::from_utf8_lossy(malformed_class_output.as_bytes()).contains(
            "preg_replace(): Compilation failed: missing terminating ] for character class at offset 8"
        ));
    let mut error_output = OutputBuffer::new();
    assert_eq!(
        call_error(
            "preg_replace",
            vec![
                Value::string("/[a-z]/"),
                Value::packed_array(vec![Value::string("x")]),
                Value::string("subject"),
            ],
            &mut error_output,
        ),
        "preg_replace(): Argument #1 ($pattern) must be of type array when argument #2 ($replacement) is an array, string given"
    );
    let replacement_object = Value::Object(ObjectRef::new_with_display_name(
        &empty_class("stdClass"),
        "stdClass",
    ));
    assert_eq!(
        call_error(
            "preg_replace",
            vec![
                Value::string("/[a-z]/"),
                replacement_object,
                Value::string("subject"),
            ],
            &mut error_output,
        ),
        "preg_replace(): Argument #2 ($replacement) must be of type array|string, stdClass given"
    );

    let array_pattern_count = ReferenceCell::new(Value::Null);
    assert_eq!(
        call_in_context(
            &mut context,
            "preg_replace",
            vec![
                Value::packed_array(vec![Value::string("/a/"), Value::string("/b/")]),
                Value::packed_array(vec![Value::string("A"), Value::string("B")]),
                Value::string("abc"),
                Value::Int(-1),
                Value::Reference(array_pattern_count.clone()),
            ],
        ),
        Value::string("ABc")
    );
    assert_eq!(array_pattern_count.get(), Value::Int(2));

    let short_replacement_count = ReferenceCell::new(Value::Null);
    assert_eq!(
        call_in_context(
            &mut context,
            "preg_replace",
            vec![
                Value::packed_array(vec![
                    Value::string("/a/"),
                    Value::string("/b/"),
                    Value::string("/c/"),
                ]),
                Value::packed_array(vec![Value::string("A")]),
                Value::string("abc"),
                Value::Int(-1),
                Value::Reference(short_replacement_count.clone()),
            ],
        ),
        Value::string("A")
    );
    assert_eq!(short_replacement_count.get(), Value::Int(3));

    let mut keyed_subject = PhpArray::new();
    keyed_subject.insert(
        ArrayKey::String(PhpString::from_test_str("first")),
        Value::string("aa"),
    );
    keyed_subject.insert(ArrayKey::Int(5), Value::string("aa"));
    let keyed_subject_count = ReferenceCell::new(Value::Null);
    let keyed_subject_result = call_in_context(
        &mut context,
        "preg_replace",
        vec![
            Value::string("/a/"),
            Value::string("A"),
            Value::Array(keyed_subject),
            Value::Int(1),
            Value::Reference(keyed_subject_count.clone()),
        ],
    );
    let Value::Array(keyed_subject_result) = keyed_subject_result else {
        panic!("expected keyed preg_replace subject array");
    };
    assert_eq!(
        keyed_subject_result.get(&ArrayKey::String(PhpString::from_test_str("first"))),
        Some(&Value::string("Aa"))
    );
    assert_eq!(
        keyed_subject_result.get(&ArrayKey::Int(5)),
        Some(&Value::string("Aa"))
    );
    assert_eq!(keyed_subject_count.get(), Value::Int(2));

    let filter_scalar_count = ReferenceCell::new(Value::Null);
    assert_eq!(
        call_in_context(
            &mut context,
            "preg_filter",
            vec![
                Value::string("/z/"),
                Value::string("Z"),
                Value::string("abc"),
                Value::Int(-1),
                Value::Reference(filter_scalar_count.clone()),
            ],
        ),
        Value::Null
    );
    assert_eq!(filter_scalar_count.get(), Value::Int(0));
    let mut filter_subject = PhpArray::new();
    filter_subject.insert(ArrayKey::Int(0), Value::string("1"));
    filter_subject.insert(ArrayKey::Int(1), Value::string("a"));
    filter_subject.insert(ArrayKey::Int(2), Value::string("B"));
    let filter_count = ReferenceCell::new(Value::Null);
    let filter_result = call_in_context(
        &mut context,
        "preg_filter",
        vec![
            Value::packed_array(vec![Value::string(r#"/\d/"#), Value::string("/[a-z]/")]),
            Value::packed_array(vec![Value::string("A:$0"), Value::string("B:$0")]),
            Value::Array(filter_subject),
            Value::Int(-1),
            Value::Reference(filter_count.clone()),
        ],
    );
    let Value::Array(filter_result) = filter_result else {
        panic!("expected keyed preg_filter subject array");
    };
    assert_eq!(filter_result.len(), 2);
    assert_eq!(
        filter_result.get(&ArrayKey::Int(0)),
        Some(&Value::string("A:1"))
    );
    assert_eq!(
        filter_result.get(&ArrayKey::Int(1)),
        Some(&Value::string("B:a"))
    );
    assert_eq!(filter_count.get(), Value::Int(2));

    assert_eq!(
        call_in_context(
            &mut context,
            "preg_replace_callback",
            vec![
                Value::string(r#"/(foo)/"#),
                Value::internal_builtin_callable("count"),
                Value::string("foo foo"),
            ],
        ),
        Value::string("2 2")
    );

    assert_eq!(
        call_in_context(
            &mut context,
            "preg_replace",
            vec![
                Value::string(r#"#(&\#x*)([0-9A-F]+);*#iu"#),
                Value::string(r#"$1$2;"#),
                Value::string(vec![b's', b'e', b'a', b'r', b'c', b'h', 0xe4]),
            ],
        ),
        Value::Null
    );
    assert_eq!(
        call_in_context(&mut context, "preg_last_error", Vec::new()),
        Value::Int(pcre::PREG_BAD_UTF8_ERROR)
    );

    assert_eq!(
        array_strings(call_in_context(
            &mut context,
            "preg_split",
            vec![
                Value::string(r#"/[,;]\s*/"#),
                Value::string("a, b; c"),
                Value::Int(-1),
                Value::Int(pcre::PREG_SPLIT_NO_EMPTY),
            ],
        )),
        ["a", "b", "c"]
    );

    assert_eq!(
        call_in_context(
            &mut context,
            "preg_split",
            vec![Value::string(r#"/a/u"#), Value::string(vec![b'a', 0xff])],
        ),
        Value::Bool(false)
    );
    assert_eq!(
        call_in_context(&mut context, "preg_last_error_msg", Vec::new()),
        Value::string("Malformed UTF-8 characters, possibly incorrectly encoded")
    );

    let input = Value::packed_array(vec![
        Value::string("src/Foo.php"),
        Value::string("README.md"),
        Value::string("tests/FooTest.php"),
    ]);
    assert_eq!(
        array_strings(call_in_context(
            &mut context,
            "preg_grep",
            vec![Value::string(r#"/\.php$/"#), input],
        )),
        ["src/Foo.php", "tests/FooTest.php"]
    );

    let mut grep_cast_output = OutputBuffer::new();
    let grep_array = Value::packed_array(vec![
        Value::string("abc"),
        Value::Array(PhpArray::new()),
        Value::Bool(false),
    ]);
    let Value::Array(grep_result) = call(
        "preg_grep",
        vec![Value::string(r#"/^A/"#), grep_array],
        &mut grep_cast_output,
    ) else {
        panic!("expected preg_grep to return array");
    };
    assert_eq!(grep_result.len(), 1);
    assert_eq!(
        grep_result.get(&ArrayKey::Int(1)),
        Some(&Value::Array(PhpArray::new()))
    );
    let grep_warning = std::str::from_utf8(grep_cast_output.as_bytes()).unwrap();
    assert!(grep_warning.contains("Warning: Array to string conversion"));

    let invalid_grep_input =
        Value::packed_array(vec![Value::string("a"), Value::string(vec![b'1', 0xff])]);
    assert_eq!(
        call_in_context(
            &mut context,
            "preg_grep",
            vec![Value::string(r#"#\d#u"#), invalid_grep_input],
        ),
        Value::Bool(false)
    );
    assert_eq!(
        call_in_context(&mut context, "preg_last_error", Vec::new()),
        Value::Int(pcre::PREG_BAD_UTF8_ERROR)
    );

    assert_eq!(
        call_in_context(
            &mut context,
            "preg_quote",
            vec![Value::string("a+b/c"), Value::string("/")],
        ),
        Value::string(r#"a\+b\/c"#)
    );

    assert_eq!(
        call_in_context(
            &mut context,
            "preg_match",
            vec![Value::string("/["), Value::string("x")],
        ),
        Value::Bool(false)
    );
    assert_eq!(
        call_in_context(&mut context, "preg_last_error", Vec::new()),
        Value::Int(pcre::PREG_INTERNAL_ERROR)
    );
    assert_eq!(
        call_in_context(&mut context, "preg_last_error_msg", Vec::new()),
        Value::string("Internal error")
    );
}

#[test]
fn date_timezone_defaults_set_and_list_are_request_local() {
    let mut output = OutputBuffer::new();
    let mut context = BuiltinContext::new(&mut output);

    assert_eq!(
        call_in_context(&mut context, "date_default_timezone_get", Vec::new()),
        Value::string("UTC")
    );
    assert_eq!(
        call_in_context(
            &mut context,
            "date_default_timezone_set",
            vec![Value::string("Europe/Berlin")],
        ),
        Value::Bool(true)
    );
    assert_eq!(
        call_in_context(&mut context, "date_default_timezone_get", Vec::new()),
        Value::string("Europe/Berlin")
    );
    assert_eq!(
        call_in_context(
            &mut context,
            "date_default_timezone_set",
            vec![Value::string("+0000")],
        ),
        Value::Bool(true)
    );
    assert_eq!(
        call_in_context(&mut context, "date_default_timezone_get", Vec::new()),
        Value::string("+00:00")
    );
    assert_eq!(
        call_in_context(
            &mut context,
            "date_default_timezone_set",
            vec![Value::string("Mars/Base")],
        ),
        Value::Bool(false)
    );
    assert_eq!(
        call_in_context(&mut context, "date_default_timezone_get", Vec::new()),
        Value::string("+00:00")
    );

    let identifiers = array_strings(call_in_context(
        &mut context,
        "timezone_identifiers_list",
        Vec::new(),
    ));
    assert!(identifiers.contains(&"UTC".to_string()));
    assert!(identifiers.contains(&"Europe/Berlin".to_string()));
}

#[test]
fn date_functions_parse_format_and_use_request_timezone() {
    let mut output = OutputBuffer::new();
    let mut context = BuiltinContext::new(&mut output);

    assert_eq!(
        call_in_context(
            &mut context,
            "date",
            vec![Value::string("Y-m-d H:i:s O"), Value::Int(0)],
        ),
        Value::string("1970-01-01 00:00:00 +0000")
    );
    assert_eq!(
        call_in_context(
            &mut context,
            "date_default_timezone_set",
            vec![Value::string("Europe/Berlin")],
        ),
        Value::Bool(true)
    );
    assert_eq!(
        call_in_context(
            &mut context,
            "date",
            vec![Value::string("Y-m-d H:i:s T"), Value::Int(0)],
        ),
        Value::string("1970-01-01 01:00:00 CET")
    );
    assert_eq!(
        call_in_context(
            &mut context,
            "gmdate",
            vec![Value::string("Y-m-d H:i:s T O P"), Value::Int(0)],
        ),
        Value::string("1970-01-01 00:00:00 GMT +0000 +00:00")
    );
    assert_eq!(
        call_in_context(
            &mut context,
            "strtotime",
            vec![Value::string("2024-01-02 03:04:05")],
        ),
        Value::Int(1_704_161_045)
    );
    assert_eq!(
        call_in_context(
            &mut context,
            "strtotime",
            vec![Value::string("2009-02-12 12:47:41 GMT")],
        ),
        Value::Int(1_234_442_861)
    );
    assert_eq!(
        call_in_context(
            &mut context,
            "strtotime",
            vec![Value::string("+2 days"), Value::Int(0)],
        ),
        Value::Int(172_800)
    );
    assert!(matches!(
        call_in_context(&mut context, "time", Vec::new()),
        Value::Int(value) if value > 0
    ));
    let Value::Array(hrtime) = call_in_context(&mut context, "hrtime", Vec::new()) else {
        panic!("hrtime() should return an array");
    };
    let entries = super::array_entries(&hrtime);
    assert_eq!(entries.len(), 2);
    assert!(matches!(entries[0].1, Value::Int(value) if value > 0));
    assert!(matches!(entries[1].1, Value::Int(value) if (0..1_000_000_000).contains(&value)));
    assert!(matches!(
        call_in_context(&mut context, "hrtime", vec![Value::Bool(true)]),
        Value::Int(value) if value > 0
    ));
}

#[test]
fn spl_object_identity_builtins_use_stable_runtime_object_ids() {
    let mut output = OutputBuffer::new();
    let object = Value::Object(ObjectRef::new(&empty_class("SplBox")));

    let Value::Int(id) = call("spl_object_id", vec![object.clone()], &mut output) else {
        panic!("expected object id int");
    };
    assert!(id > 0);
    assert_eq!(
        call("spl_object_id", vec![object.clone()], &mut output),
        Value::Int(id)
    );
    assert_eq!(
        call("spl_object_hash", vec![object], &mut output),
        Value::string(format!("{id:032x}"))
    );

    let closure = Value::closure(ClosurePayload::new(1, Vec::new()));
    let Value::Int(closure_id) = call("spl_object_id", vec![closure.clone()], &mut output) else {
        panic!("expected closure object id int");
    };
    assert!(closure_id > 0);
    assert_eq!(
        call("spl_object_hash", vec![closure], &mut output),
        Value::string(format!("{closure_id:032x}"))
    );
}

#[test]
fn datetime_objects_cover_mutable_immutable_interval_and_diff_mvp() {
    let Value::Object(datetime) = datetime::datetime_object(0, "UTC") else {
        panic!("expected DateTime object");
    };
    assert_eq!(datetime.class_name(), "datetime");
    assert_eq!(datetime.display_name(), "DateTime");
    assert_eq!(
        datetime::format_timestamp(
            datetime::object_timestamp(&datetime).expect("timestamp"),
            &datetime::object_timezone(&datetime).expect("timezone"),
            "Y-m-d H:i:s"
        ),
        "1970-01-01 00:00:00"
    );

    let updated = datetime::with_timestamp(&datetime, 60, false);
    assert!(matches!(updated, Value::Object(_)));
    assert_eq!(datetime::object_timestamp(&datetime), Some(60));

    let Value::Object(immutable) = datetime::datetime_immutable_object(0, "UTC") else {
        panic!("expected DateTimeImmutable object");
    };
    let changed = datetime::with_timestamp(&immutable, 60, true);
    let Value::Object(changed) = changed else {
        panic!("expected changed immutable object");
    };
    assert_eq!(datetime::object_timestamp(&immutable), Some(0));
    assert_eq!(datetime::object_timestamp(&changed), Some(60));
    assert_eq!(changed.class_name(), "datetimeimmutable");
    assert_eq!(changed.display_name(), "DateTimeImmutable");

    let interval_seconds = datetime::parse_interval_spec("P1DT2H").expect("interval");
    assert_eq!(interval_seconds, 93_600);
    let added = datetime::add_interval(&immutable, interval_seconds, true);
    let Value::Object(added) = added else {
        panic!("expected DateTimeImmutable after add");
    };
    assert_eq!(datetime::object_timestamp(&added), Some(93_600));
    let diff = datetime::diff_objects(&immutable, &added);
    let Value::Object(diff) = diff else {
        panic!("expected DateInterval object");
    };
    assert_eq!(diff.class_name(), "dateinterval");
    assert_eq!(diff.display_name(), "DateInterval");
    assert_eq!(diff.get_property("__seconds"), Some(Value::Int(93_600)));

    let modified = datetime::modify_object(&immutable, "+1 day", true).expect("modify");
    let Value::Object(modified) = modified else {
        panic!("expected modified object");
    };
    assert_eq!(datetime::object_timestamp(&modified), Some(86_400));
    assert!(datetime::modify_object(&immutable, "next tuesday", true).is_none());
}

#[test]
fn json_builtins_cover_composer_style_documents_and_modes() {
    let mut output = OutputBuffer::new();
    let mut context = BuiltinContext::new(&mut output);

    let decoded = call_in_context(
        &mut context,
        "json_decode",
        vec![
            Value::string(r#"{"name":"pkg","autoload":{"psr-4":{"App\\":"src/"}}}"#),
            Value::Bool(true),
        ],
    );
    let Value::Array(root) = decoded else {
        panic!("expected associative json array");
    };
    assert_eq!(
        root.get(&ArrayKey::String(PhpString::from_test_str("name"))),
        Some(&Value::string("pkg"))
    );
    assert!(matches!(
        root.get(&ArrayKey::String(PhpString::from_test_str("autoload"))),
        Some(Value::Array(_))
    ));

    let object = call_in_context(
        &mut context,
        "json_decode",
        vec![Value::string(r#"{"answer":42}"#)],
    );
    let Value::Object(object) = object else {
        panic!("expected stdClass object");
    };
    assert_eq!(object.class_name(), "stdclass");
    assert_eq!(object.display_name(), "stdClass");
    assert_eq!(object.get_property("answer"), Some(Value::Int(42)));

    let decoded_with_flag = call_in_context(
        &mut context,
        "json_decode",
        vec![
            Value::string(r#"{"answer":42}"#),
            Value::Null,
            Value::Int(512),
            Value::Int(JSON_OBJECT_AS_ARRAY),
        ],
    );
    assert!(matches!(decoded_with_flag, Value::Array(_)));

    let mut mixed = crate::PhpArray::new();
    mixed.insert(
        ArrayKey::String(PhpString::from_test_str("name")),
        Value::string("pkg"),
    );
    mixed.insert(
        ArrayKey::String(PhpString::from_test_str("versions")),
        Value::packed_array(vec![Value::string("1.0.0"), Value::string("1.1.0")]),
    );
    assert_eq!(
        call_in_context(&mut context, "json_encode", vec![Value::Array(mixed)]),
        Value::string(r#"{"name":"pkg","versions":["1.0.0","1.1.0"]}"#)
    );
    let mut ordered = crate::PhpArray::new();
    ordered.insert(
        ArrayKey::String(PhpString::from_test_str("url")),
        Value::string("https://example.test/a"),
    );
    ordered.insert(
        ArrayKey::String(PhpString::from_test_str("snow")),
        Value::string("☃"),
    );
    ordered.insert(
        ArrayKey::String(PhpString::from_test_str("n")),
        Value::float(1.0),
    );
    assert_eq!(
        call_in_context(
            &mut context,
            "json_encode",
            vec![Value::Array(ordered.clone())]
        ),
        Value::string(r#"{"url":"https:\/\/example.test\/a","snow":"\u2603","n":1}"#)
    );
    assert_eq!(
        call_in_context(
            &mut context,
            "json_encode",
            vec![
                Value::Array(ordered),
                Value::Int(JSON_UNESCAPED_SLASHES | JSON_UNESCAPED_UNICODE)
            ]
        ),
        Value::string(r#"{"url":"https://example.test/a","snow":"☃","n":1}"#)
    );
    assert_eq!(
        call_in_context(
            &mut context,
            "json_encode",
            vec![
                Value::packed_array(vec![Value::Int(1)]),
                Value::Int(JSON_FORCE_OBJECT)
            ]
        ),
        Value::string(r#"{"0":1}"#)
    );
    let hex_flags = JSON_HEX_TAG | JSON_HEX_AMP | JSON_HEX_APOS | JSON_HEX_QUOT;
    assert_eq!(
        call_in_context(
            &mut context,
            "json_encode",
            vec![Value::string("<tag>&'\""), Value::Int(hex_flags)]
        ),
        Value::string(r#""\u003Ctag\u003E\u0026\u0027\u0022""#)
    );
    assert_eq!(
        call_in_context(
            &mut context,
            "json_encode",
            vec![Value::string("9.4324"), Value::Int(JSON_NUMERIC_CHECK)]
        ),
        Value::string("9.4324")
    );
    assert_eq!(
        call_in_context(
            &mut context,
            "json_encode",
            vec![
                Value::packed_array(vec![Value::string("122321"), Value::string("plain")]),
                Value::Int(JSON_NUMERIC_CHECK)
            ]
        ),
        Value::string(r#"[122321,"plain"]"#)
    );
    assert_eq!(
        call_in_context(&mut context, "json_encode", vec![Value::float(42.0)]),
        Value::string("42")
    );
    let flags = JSON_PRETTY_PRINT
        | JSON_UNESCAPED_SLASHES
        | JSON_UNESCAPED_UNICODE
        | JSON_PRESERVE_ZERO_FRACTION;
    let encoded_with_flags = call_in_context(
        &mut context,
        "json_encode",
        vec![
            Value::packed_array(vec![
                Value::string("https://example.test/ü"),
                Value::float(1.0),
            ]),
            Value::Int(flags),
        ],
    );
    let Value::String(encoded_with_flags) = encoded_with_flags else {
        panic!("expected encoded JSON string");
    };
    let encoded_with_flags = encoded_with_flags.to_string_lossy();
    assert!(encoded_with_flags.contains('\n'));
    assert!(encoded_with_flags.contains("\n    \"https://example.test/ü\""));
    assert!(encoded_with_flags.contains("https://example.test/ü"));
    assert!(encoded_with_flags.contains("1.0"));
    assert_eq!(
        call_in_context(&mut context, "json_last_error", Vec::new()),
        Value::Int(JSON_ERROR_NONE)
    );
    assert_eq!(
        call_in_context(
            &mut context,
            "json_decode",
            vec![
                Value::string(r#"[123456789012345678901234567890]"#),
                Value::Null,
                Value::Int(512),
                Value::Int(JSON_BIGINT_AS_STRING)
            ]
        ),
        Value::packed_array(vec![Value::string("123456789012345678901234567890")])
    );
    assert_eq!(
        call_in_context(
            &mut context,
            "json_decode",
            vec![Value::string("[[1]]"), Value::Null, Value::Int(2)]
        ),
        Value::Null
    );
    assert_eq!(
        call_in_context(&mut context, "json_last_error", Vec::new()),
        Value::Int(JSON_ERROR_DEPTH)
    );
    assert_eq!(
        call_in_context(&mut context, "json_decode", vec![Value::string("[1}")]),
        Value::Null
    );
    assert_eq!(
        call_in_context(&mut context, "json_last_error", Vec::new()),
        Value::Int(JSON_ERROR_STATE_MISMATCH)
    );
    assert_eq!(
        call_in_context(&mut context, "json_decode", vec![Value::string("\"a\0b\"")]),
        Value::Null
    );
    assert_eq!(
        call_in_context(&mut context, "json_last_error", Vec::new()),
        Value::Int(JSON_ERROR_CTRL_CHAR)
    );
    assert_eq!(
        call_in_context(
            &mut context,
            "json_validate",
            vec![Value::string("[1,2,3]")]
        ),
        Value::Bool(true)
    );
}

#[test]
fn json_errors_are_recorded_and_throw_flag_errors() {
    let mut output = OutputBuffer::new();
    let mut context = BuiltinContext::new(&mut output);

    assert_eq!(
        call_in_context(&mut context, "json_decode", vec![Value::string("{")]),
        Value::Null
    );
    assert_eq!(
        call_in_context(&mut context, "json_last_error", Vec::new()),
        Value::Int(JSON_ERROR_SYNTAX)
    );
    assert_eq!(
        call_in_context(&mut context, "json_last_error_msg", Vec::new()),
        Value::string("Syntax error")
    );
    assert_eq!(
        call_in_context(&mut context, "json_validate", vec![Value::string("{")]),
        Value::Bool(false)
    );

    let entry = BuiltinRegistry::new()
        .get("json_decode")
        .expect("json_decode exists");
    let result = (entry.function())(
        &mut context,
        vec![
            Value::string("{"),
            Value::Null,
            Value::Int(512),
            Value::Int(JSON_THROW_ON_ERROR),
        ],
        RuntimeSourceSpan::default(),
    );
    assert!(matches!(
        result,
        Err(error) if error.diagnostic_id() == "E_PHP_RUNTIME_JSON_EXCEPTION"
    ));
}

#[test]
fn json_encode_enum_cases_match_php_error_and_backing_value() {
    let mut output = OutputBuffer::new();
    let mut context = BuiltinContext::new(&mut output);

    let unit = ObjectRef::new(&enum_class("unitenumcase", None));
    unit.set_property("name", Value::string("A"));
    assert_eq!(
        call_in_context(
            &mut context,
            "json_encode",
            vec![Value::Object(unit.clone())]
        ),
        Value::Bool(false)
    );
    assert_eq!(
        call_in_context(&mut context, "json_last_error", Vec::new()),
        Value::Int(JSON_ERROR_NON_BACKED_ENUM)
    );
    assert_eq!(
        call_in_context(&mut context, "json_last_error_msg", Vec::new()),
        Value::string("Non-backed enums have no default serialization")
    );
    assert_eq!(
        call_in_context(
            &mut context,
            "json_encode",
            vec![
                Value::Object(unit),
                Value::Int(JSON_PARTIAL_OUTPUT_ON_ERROR)
            ]
        ),
        Value::string("0")
    );
    assert_eq!(
        call_in_context(&mut context, "json_last_error", Vec::new()),
        Value::Int(JSON_ERROR_NON_BACKED_ENUM)
    );

    let backed = ObjectRef::new(&enum_class(
        "backedenumcase",
        Some(ClassEnumBackingType::String),
    ));
    backed.set_property("name", Value::string("A"));
    backed.set_property("value", Value::string("x"));
    assert_eq!(
        call_in_context(&mut context, "json_encode", vec![Value::Object(backed)]),
        Value::string(r#""x""#)
    );
    assert_eq!(
        call_in_context(&mut context, "json_last_error", Vec::new()),
        Value::Int(JSON_ERROR_NONE)
    );
}

#[test]
fn symlink_stat_is_conditional_on_platform_support() {
    let root = std::env::temp_dir().join(format!("phrust-stdlib-lstat-{}", std::process::id()));
    std::fs::create_dir_all(&root).expect("create temp root");
    let target = root.join("target.txt");
    let link = root.join("link.txt");
    std::fs::write(&target, b"target").expect("write target");

    #[cfg(unix)]
    std::os::unix::fs::symlink(&target, &link).expect("create symlink");
    #[cfg(windows)]
    {
        if std::os::windows::fs::symlink_file(&target, &link).is_err() {
            let _ = std::fs::remove_file(target);
            let _ = std::fs::remove_dir(root);
            return;
        }
    }

    let mut output = OutputBuffer::new();
    let capabilities = FilesystemCapabilities::none().with_allowed_roots(vec![root.clone()]);
    assert_eq!(
        call_with_fs(
            "is_link",
            vec![Value::string("link.txt")],
            &mut output,
            root.clone(),
            capabilities.clone()
        ),
        Value::Bool(true)
    );
    assert!(matches!(
        call_with_fs(
            "lstat",
            vec![Value::string("link.txt")],
            &mut output,
            root.clone(),
            capabilities
        ),
        Value::Array(_)
    ));

    let _ = std::fs::remove_file(link);
    let _ = std::fs::remove_file(target);
    let _ = std::fs::remove_dir(root);
}

fn empty_class(name: &str) -> ClassEntry {
    ClassEntry {
        name: normalize_class_name(name).into(),
        parent: None,
        interfaces: Vec::new(),
        methods: Vec::new(),
        properties: Vec::new(),
        constants: Vec::new(),
        enum_cases: Vec::new(),
        attributes: Vec::new(),
        enum_backing_type: None,
        constructor_id: None,
        flags: ClassFlags::default(),
    }
}

fn enum_class(name: &str, backing: Option<ClassEnumBackingType>) -> ClassEntry {
    let mut properties = vec![ClassPropertyEntry {
        name: "name".to_owned(),
        default: Value::Uninitialized,
        type_: Some(RuntimeType::String),
        flags: ClassPropertyFlags {
            is_readonly: true,
            is_typed: true,
            ..ClassPropertyFlags::default()
        },
        hooks: ClassPropertyHooks::default(),
        attributes: Vec::new(),
    }];
    if backing.is_some() {
        properties.push(ClassPropertyEntry {
            name: "value".to_owned(),
            default: Value::Uninitialized,
            type_: Some(RuntimeType::String),
            flags: ClassPropertyFlags {
                is_readonly: true,
                is_typed: true,
                ..ClassPropertyFlags::default()
            },
            hooks: ClassPropertyHooks::default(),
            attributes: Vec::new(),
        });
    }
    ClassEntry {
        name: normalize_class_name(name).into(),
        parent: None,
        interfaces: Vec::new(),
        methods: Vec::new(),
        properties,
        constants: Vec::new(),
        enum_cases: Vec::new(),
        attributes: Vec::new(),
        enum_backing_type: backing,
        constructor_id: None,
        flags: ClassFlags {
            is_enum: true,
            ..ClassFlags::default()
        },
    }
}

#[test]
fn builtins_var_dump_is_stable_for_scalars_and_arrays() {
    let mut output = OutputBuffer::new();
    let result = call(
        "var_dump",
        vec![
            Value::Null,
            Value::Bool(true),
            Value::Int(7),
            Value::float(1.0),
            Value::float(1.7000000000000002),
            Value::float(3.9000000000000004),
            Value::float(4.2),
            Value::float(f64::INFINITY),
            Value::float(f64::NAN),
            Value::float(9_223_372_036_854_776_000.0),
            Value::string("hi"),
            Value::packed_array(vec![Value::Int(1), Value::string("x")]),
        ],
        &mut output,
    );

    assert_eq!(result, Value::Null);
    assert_eq!(
        output.to_string_lossy(),
        "NULL\nbool(true)\nint(7)\nfloat(1)\nfloat(1.7000000000000002)\nfloat(3.9000000000000004)\nfloat(4.2)\nfloat(INF)\nfloat(NAN)\nfloat(9.223372036854776E+18)\nstring(2) \"hi\"\narray(2) {\n  [0]=>\n  int(1)\n  [1]=>\n  string(1) \"x\"\n}\n"
    );
}

#[test]
fn var_dump_marks_array_references_to_active_arrays_as_recursion() {
    let cell = ReferenceCell::new(Value::Null);
    let mut array = PhpArray::new();
    array.append(Value::Reference(cell.clone()));
    cell.set(Value::Array(array.clone()));

    let mut output = OutputBuffer::new();
    let result = call("var_dump", vec![Value::Array(array)], &mut output);

    assert_eq!(result, Value::Null);
    assert_eq!(
        output.to_string_lossy(),
        "array(1) {\n  [0]=>\n  *RECURSION*\n}\n"
    );
}

#[test]
fn var_dump_marks_object_references_to_active_objects_as_recursion() {
    let object = ObjectRef::new(&empty_class("DebugBox"));
    let cell = ReferenceCell::new(Value::Object(object.clone()));
    object.set_property("self", Value::Reference(cell));

    let mut output = OutputBuffer::new();
    let result = call("var_dump", vec![Value::Object(object)], &mut output);

    assert_eq!(result, Value::Null);
    assert!(output.to_string_lossy().contains("*RECURSION*\n"));
}

#[test]
fn var_dump_prints_callable_closure_metadata() {
    let mut output = OutputBuffer::new();
    let result = call(
        "var_dump",
        vec![
            Value::user_function_callable("test1"),
            Value::closure(crate::ClosurePayload::new(3, Vec::new()).with_debug(Some(
                ClosureDebugInfo {
                    name: "{closure:/tmp/source.php:7}".to_owned(),
                    file: "/tmp/source.php".to_owned(),
                    line: 7,
                    parameters: vec![crate::ClosureDebugParameter {
                        name: "class".to_owned(),
                        required: true,
                    }],
                },
            ))),
            Value::closure(
                crate::ClosurePayload::new(
                    4,
                    vec![crate::ClosureCaptureValue::by_value(
                        "x".to_owned(),
                        Value::Int(2),
                    )],
                )
                .with_debug(Some(ClosureDebugInfo {
                    name: "{closure:/tmp/source.php:9}".to_owned(),
                    file: "/tmp/source.php".to_owned(),
                    line: 9,
                    parameters: Vec::new(),
                })),
            ),
        ],
        &mut output,
    );

    assert_eq!(result, Value::Null);
    let dumped = output.to_string_lossy();
    let closure_headers = dumped
        .lines()
        .filter(|line| line.starts_with("object(Closure)#"))
        .collect::<Vec<_>>();
    assert_eq!(closure_headers.len(), 3);
    assert_eq!(closure_headers[0], "object(Closure)#1 (1) {");
    assert!(closure_headers[1].ends_with(" (4) {"));
    assert!(closure_headers[2].ends_with(" (4) {"));
    assert_ne!(
        closure_debug_id(closure_headers[1]),
        closure_debug_id(closure_headers[2])
    );
    assert!(dumped.contains("string(27) \"{closure:/tmp/source.php:7}\""));
    assert!(dumped.contains("string(27) \"{closure:/tmp/source.php:9}\""));
    assert!(dumped.contains("[\"parameter\"]=>\n  array(1) {"));
    assert!(dumped.contains("[\"$class\"]=>\n    string(10) \"<required>\""));
    assert!(dumped.contains("[\"static\"]=>\n  array(1) {"));
}

fn closure_debug_id(header: &str) -> &str {
    header
        .split_once('#')
        .and_then(|(_, rest)| rest.split_once(' '))
        .map(|(id, _)| id)
        .expect("closure var_dump header should include an object handle")
}

#[test]
fn var_dump_orders_closure_debug_fields_like_reference_php() {
    // Reference PHP 8.5 emits name, file, line, static, this, parameter.
    // Keep this asserted without the reference oracle so CI catches a
    // reorder even when REFERENCE_PHP is unavailable.
    let mut output = OutputBuffer::new();
    let bound_this = ObjectRef::new(&empty_class("BoundTarget"));
    call(
        "var_dump",
        vec![Value::closure(
            crate::ClosurePayload::new(
                11,
                vec![crate::ClosureCaptureValue::by_value(
                    "captured".to_owned(),
                    Value::Int(5),
                )],
            )
            .with_bound_this(Some(bound_this))
            .with_debug(Some(ClosureDebugInfo {
                name: "{closure:/tmp/order.php:3}".to_owned(),
                file: "/tmp/order.php".to_owned(),
                line: 3,
                parameters: vec![crate::ClosureDebugParameter {
                    name: "p".to_owned(),
                    required: true,
                }],
            })),
        )],
        &mut output,
    );

    let dumped = output.to_string_lossy();
    let order = [
        "[\"name\"]",
        "[\"file\"]",
        "[\"line\"]",
        "[\"static\"]",
        "[\"this\"]",
        "[\"parameter\"]",
    ]
    .map(|field| {
        dumped
            .find(field)
            .unwrap_or_else(|| panic!("{field} missing from closure var_dump:\n{dumped}"))
    });
    assert!(
        order.windows(2).all(|pair| pair[0] < pair[1]),
        "closure var_dump fields out of reference order:\n{dumped}"
    );
}

#[test]
fn print_r_marks_array_references_to_active_arrays_as_recursion() {
    let outer_cell = ReferenceCell::new(Value::Null);
    let inner_cell = ReferenceCell::new(Value::Null);
    let mut inner = PhpArray::new();
    inner.append(Value::Reference(outer_cell.clone()));
    inner_cell.set(Value::Array(inner.clone()));
    let mut outer = PhpArray::new();
    outer.append(Value::Reference(inner_cell));
    outer_cell.set(Value::Array(outer.clone()));

    let mut output = OutputBuffer::new();
    let result = call("print_r", vec![Value::Array(outer)], &mut output);

    assert_eq!(result, Value::Bool(true));
    assert_eq!(
        output.to_string_lossy(),
        "Array\n(\n    [0] => Array\n        (\n            [0] => Array\n *RECURSION*\n        )\n\n)\n"
    );
}

#[test]
fn debug_output_builtins_cover_return_modes_and_cycles() {
    let mut output = OutputBuffer::new();

    assert_eq!(
        call(
            "print_r",
            vec![Value::packed_array(vec![Value::Int(1)]), Value::Bool(true)],
            &mut output
        ),
        Value::string("Array\n(\n    [0] => 1\n)\n")
    );
    assert_eq!(
        call(
            "print_r",
            vec![
                Value::packed_array(vec![Value::packed_array(vec![Value::Int(1)])]),
                Value::Bool(true)
            ],
            &mut output
        ),
        Value::string(
            "Array\n(\n    [0] => Array\n        (\n            [0] => 1\n        )\n\n)\n"
        )
    );
    let object = ObjectRef::new_with_display_name(&empty_class("A"), "A");
    let mut property_array = PhpArray::new();
    property_array.insert(ArrayKey::Int(1), Value::string("foo1_value"));
    property_array.insert(ArrayKey::Int(2), Value::string("foo2_value"));
    object.set_property("a_var", Value::Array(property_array));
    assert_eq!(
        call(
            "print_r",
            vec![Value::Object(object), Value::Bool(true)],
            &mut output
        ),
        Value::string(
            "A Object\n(\n    [a_var] => Array\n        (\n            [1] => foo1_value\n            [2] => foo2_value\n        )\n\n)\n"
        )
    );
    assert_eq!(
        call(
            "var_export",
            vec![
                Value::packed_array(vec![Value::string("x")]),
                Value::Bool(true)
            ],
            &mut output
        ),
        Value::string("array (\n  0 => 'x',\n)")
    );
    assert_eq!(
        call(
            "var_export",
            vec![
                Value::packed_array(vec![Value::packed_array(vec![Value::Int(1)])]),
                Value::Bool(true)
            ],
            &mut output
        ),
        Value::string("array (\n  0 => \n  array (\n    0 => 1,\n  ),\n)")
    );
    let mut nul_key_array = PhpArray::new();
    nul_key_array.insert(
        ArrayKey::String(PhpString::from_bytes(vec![0])),
        Value::string("null"),
    );
    assert_eq!(
        call(
            "var_export",
            vec![Value::Array(nul_key_array), Value::Bool(true)],
            &mut output
        ),
        Value::string("array (\n  '' . \"\\0\" . '' => 'null',\n)")
    );
    assert_eq!(
        call(
            "var_export",
            vec![Value::float(1.0), Value::Bool(true)],
            &mut output
        ),
        Value::string("1.0")
    );
    assert_eq!(
        call(
            "var_export",
            vec![Value::float(-0.0), Value::Bool(true)],
            &mut output
        ),
        Value::string("-0.0")
    );
    assert_eq!(
        call(
            "var_export",
            vec![Value::float(10_000_000_000_000_000.0), Value::Bool(true)],
            &mut output
        ),
        Value::string("10000000000000000.0")
    );
    let std_class = ObjectRef::new(&empty_class("stdClass"));
    std_class.set_property("0", Value::Int(1));
    std_class.set_property("foo", Value::packed_array(vec![Value::Int(2)]));
    assert_eq!(
        call(
            "var_export",
            vec![Value::Object(std_class), Value::Bool(true)],
            &mut output
        ),
        Value::string(
            "(object) array(\n   '0' => 1,\n   'foo' => \n  array (\n    0 => 2,\n  ),\n)"
        )
    );
    let debug_box = ObjectRef::new_with_display_name(&empty_class("DebugBox"), "DebugBox");
    debug_box.set_property("x", Value::Int(1));
    assert_eq!(
        call(
            "var_export",
            vec![Value::Object(debug_box), Value::Bool(true)],
            &mut output
        ),
        Value::string("\\DebugBox::__set_state(array(\n   'x' => 1,\n))")
    );
    let fixed_array =
        ObjectRef::new_with_display_name(&empty_class("MySplFixedArray"), "MySplFixedArray");
    fixed_array.set_property("__spl_runtime_class", Value::string("splfixedarray"));
    fixed_array.set_property(
        "__entries",
        Value::packed_array(vec![Value::packed_array(vec![
            Value::Int(0),
            Value::Object(fixed_array.clone()),
        ])]),
    );
    assert_eq!(
        call(
            "var_export",
            vec![Value::Object(fixed_array.clone()), Value::Bool(true)],
            &mut output
        ),
        Value::string("\\MySplFixedArray::__set_state(array(\n   0 => NULL,\n))")
    );
    assert!(
        output
            .to_string_lossy()
            .contains("var_export does not handle circular references")
    );
    let debug_start = output.as_bytes().len();
    assert_eq!(
        call(
            "debug_zval_dump",
            vec![Value::Object(fixed_array)],
            &mut output
        ),
        Value::Null
    );
    let output_text = output.to_string_lossy();
    let debug_output = &output_text[debug_start..];
    assert!(debug_output.contains("object(MySplFixedArray)#"));
    assert!(debug_output.contains("  [0]=>\n  *RECURSION*\n"));
    assert!(!debug_output.contains("__entries"));

    let cell = ReferenceCell::new(Value::Null);
    let mut array = PhpArray::new();
    array.append(Value::Reference(cell.clone()));
    cell.set(Value::Array(array));

    let result = call("var_dump", vec![Value::Reference(cell)], &mut output);
    assert_eq!(result, Value::Null);
    assert!(output.to_string_lossy().contains("*RECURSION*"));
}

#[test]
fn version_compare_covers_platform_check_semantics() {
    let mut output = OutputBuffer::new();

    assert_eq!(
        call(
            "version_compare",
            vec![Value::string("8.5.7"), Value::string("8.5.0")],
            &mut output
        ),
        Value::Int(1)
    );
    assert_eq!(
        call(
            "version_compare",
            vec![
                Value::string("8.5.7"),
                Value::string("8.5.7"),
                Value::string("eq")
            ],
            &mut output
        ),
        Value::Bool(true)
    );
    assert_eq!(
        call(
            "version_compare",
            vec![
                Value::string("8.5.7-dev"),
                Value::string("8.5.7"),
                Value::string("<")
            ],
            &mut output
        ),
        Value::Bool(true)
    );
    assert_eq!(
        call(
            "version_compare",
            vec![
                Value::string("8.5.7RC1"),
                Value::string("8.5.7"),
                Value::string("lt")
            ],
            &mut output
        ),
        Value::Bool(true)
    );
    assert_eq!(
        call(
            "version_compare",
            vec![
                Value::string("8.5.7pl1"),
                Value::string("8.5.7"),
                Value::string("gt")
            ],
            &mut output
        ),
        Value::Bool(true)
    );
}

#[test]
fn string_search_and_compare_builtins_are_binary_safe() {
    let mut output = OutputBuffer::new();

    assert_eq!(
        call("strlen", vec![Value::string(b"a\0b".to_vec())], &mut output),
        Value::Int(3)
    );
    assert_eq!(
        call(
            "substr",
            vec![Value::string("abcdef"), Value::Int(-3), Value::Int(2)],
            &mut output
        ),
        Value::string("de")
    );
    assert_eq!(
        call(
            "strpos",
            vec![
                Value::string(b"a\0b\0c".to_vec()),
                Value::string(b"\0b".to_vec())
            ],
            &mut output
        ),
        Value::Int(1)
    );
    assert_eq!(
        call(
            "stripos",
            vec![Value::string("AbCd"), Value::string("bc")],
            &mut output
        ),
        Value::Int(1)
    );
    assert_eq!(
        call(
            "strrpos",
            vec![Value::string("abcabc"), Value::string("a"), Value::Int(-1)],
            &mut output
        ),
        Value::Int(3)
    );
    assert_eq!(
        call(
            "strrpos",
            vec![Value::string("abcabc"), Value::string("a"), Value::Int(2)],
            &mut output
        ),
        Value::Int(3)
    );
    assert_eq!(
        call(
            "strrpos",
            vec![
                Value::string("abcabc"),
                Value::string("abcabc"),
                Value::Int(1)
            ],
            &mut output
        ),
        Value::Bool(false)
    );
    assert_eq!(
        call(
            "strrpos",
            vec![Value::string("abcabc"), Value::string("a"), Value::Int(-4)],
            &mut output
        ),
        Value::Int(0)
    );
    assert_eq!(
        call(
            "strrpos",
            vec![Value::string("abc"), Value::string("")],
            &mut output
        ),
        Value::Int(3)
    );
    assert_eq!(
        call(
            "strrpos",
            vec![Value::string("abc"), Value::string(""), Value::Int(-1)],
            &mut output
        ),
        Value::Int(2)
    );
    assert_eq!(
        call_error(
            "strrpos",
            vec![Value::string("abc"), Value::string("a"), Value::Int(10)],
            &mut output
        ),
        "strrpos(): Argument #3 ($offset) must be contained in argument #1 ($haystack)"
    );
    assert_eq!(
        call_error(
            "strrpos",
            vec![
                Value::string("abc"),
                Value::string("a"),
                Value::float(f64::INFINITY)
            ],
            &mut output
        ),
        "strrpos(): Argument #3 ($offset) must be of type int, float given"
    );
    assert_eq!(
        call(
            "strripos",
            vec![Value::string("AbCaBc"), Value::string("bc")],
            &mut output
        ),
        Value::Int(4)
    );
    assert_eq!(
        call(
            "strrchr",
            vec![Value::string("abcabc"), Value::string("ab")],
            &mut output
        ),
        Value::string("abc")
    );
    assert_eq!(
        call(
            "strrchr",
            vec![Value::string("Hello, World"), Value::string("World")],
            &mut output
        ),
        Value::string("World")
    );
    assert_eq!(
        call(
            "strrchr",
            vec![
                Value::string("Hello, World"),
                Value::string("World"),
                Value::Bool(true)
            ],
            &mut output
        ),
        Value::string("Hello, ")
    );
    assert_eq!(
        call(
            "strrchr",
            vec![Value::string(b"Hello\0World".to_vec()), Value::string("")],
            &mut output
        ),
        Value::string(b"\0World".to_vec())
    );
    assert_eq!(
        call(
            "strrchr",
            vec![
                Value::string(b"Hello\0World".to_vec()),
                Value::string(""),
                Value::Bool(true)
            ],
            &mut output
        ),
        Value::string("Hello")
    );
    assert_eq!(
        call(
            "strstr",
            vec![
                Value::string("abcabc"),
                Value::string("bc"),
                Value::Bool(true)
            ],
            &mut output
        ),
        Value::string("a")
    );
    assert_eq!(
        call(
            "strstr",
            vec![Value::string("abc"), Value::string("")],
            &mut output
        ),
        Value::string("abc")
    );
    assert_eq!(
        call(
            "strstr",
            vec![Value::string("abc"), Value::string(""), Value::Bool(true)],
            &mut output
        ),
        Value::string("")
    );
    assert_eq!(
        call(
            "stristr",
            vec![Value::string("AbCaBc"), Value::string("bc")],
            &mut output
        ),
        Value::string("bCaBc")
    );
    assert_eq!(
        call(
            "stristr",
            vec![Value::string("AbC"), Value::string("")],
            &mut output
        ),
        Value::string("AbC")
    );
    assert_eq!(
        call_error(
            "stristr",
            vec![Value::string("abc"), Value::Array(PhpArray::new())],
            &mut output
        ),
        "stristr(): Argument #2 ($needle) must be of type string, array given"
    );
    let mut resources = ResourceTable::new();
    let stream = resources.register_stream(
        StreamFlags::new(true, false, true),
        StreamMetadata::new("plainfile", "stream", "r", "/tmp/example.php"),
    );
    assert_eq!(
        call_error(
            "stristr",
            vec![Value::string("abc"), Value::Resource(stream)],
            &mut output
        ),
        "stristr(): Argument #2 ($needle) must be of type string, resource given"
    );
    assert_eq!(
        call(
            "strpbrk",
            vec![Value::string("abc"), Value::string("cb")],
            &mut output
        ),
        Value::string("bc")
    );
    assert_eq!(
        call_error(
            "strpbrk",
            vec![Value::string("abc"), Value::string("")],
            &mut output
        ),
        "strpbrk(): Argument #2 ($characters) must be a non-empty string"
    );
    assert_eq!(
        call(
            "substr_count",
            vec![Value::string("aaaa"), Value::string("aa")],
            &mut output
        ),
        Value::Int(2)
    );
    assert_eq!(
        call(
            "substr_count",
            vec![
                Value::string("abcabc"),
                Value::string("bc"),
                Value::Int(0),
                Value::Null
            ],
            &mut output
        ),
        Value::Int(2)
    );
    assert_eq!(
        call_error(
            "substr_count",
            vec![Value::string("abc"), Value::string("")],
            &mut output
        ),
        "substr_count(): Argument #2 ($needle) must not be empty"
    );
    assert_eq!(
        call_error(
            "substr_count",
            vec![Value::string("abc"), Value::string("a"), Value::Int(10)],
            &mut output
        ),
        "substr_count(): Argument #3 ($offset) must be contained in argument #1 ($haystack)"
    );
    assert_eq!(
        call_error(
            "substr_count",
            vec![
                Value::string("abc"),
                Value::string("a"),
                Value::Int(1),
                Value::Int(10)
            ],
            &mut output
        ),
        "substr_count(): Argument #4 ($length) must be contained in argument #1 ($haystack)"
    );
    assert_eq!(
        call(
            "substr_compare",
            vec![
                Value::string("abc"),
                Value::string("BC"),
                Value::Int(1),
                Value::Int(2),
                Value::Bool(true)
            ],
            &mut output
        ),
        Value::Int(0)
    );
    assert_eq!(
        call(
            "substr_compare",
            vec![
                Value::string("abcde"),
                Value::string("df"),
                Value::Int(-2),
                Value::Null
            ],
            &mut output
        ),
        Value::Int(-1)
    );
    assert_eq!(
        call(
            "substr_compare",
            vec![
                Value::string("abcde"),
                Value::string("abcdef"),
                Value::Int(-10),
                Value::Int(10)
            ],
            &mut output
        ),
        Value::Int(-1)
    );
    assert_eq!(
        call_error(
            "substr_compare",
            vec![
                Value::string("abcde"),
                Value::string("abc"),
                Value::Int(0),
                Value::Int(-1)
            ],
            &mut output
        ),
        "substr_compare(): Argument #4 ($length) must be greater than or equal to 0"
    );
    assert_eq!(
        call_error(
            "strncmp",
            vec![Value::string("a"), Value::string("b"), Value::Int(-1)],
            &mut output
        ),
        "strncmp(): Argument #3 ($length) must be greater than or equal to 0"
    );
    assert_eq!(
        call_error(
            "strncasecmp",
            vec![Value::string("a"), Value::string("b"), Value::Int(-1)],
            &mut output
        ),
        "strncasecmp(): Argument #3 ($length) must be greater than or equal to 0"
    );
    assert_eq!(
        call(
            "strncasecmp",
            vec![
                Value::string(b"Hello\0world".to_vec()),
                Value::string(b"Hello\0".to_vec()),
                Value::Int(12)
            ],
            &mut output
        ),
        Value::Int(1)
    );
    assert_eq!(
        call(
            "strncasecmp",
            vec![
                Value::string(b"Hello,\0world".to_vec()),
                Value::string("Hello,world"),
                Value::Int(12)
            ],
            &mut output
        ),
        Value::Int(-119)
    );
    assert_eq!(
        call(
            "strspn",
            vec![
                Value::string("abc123"),
                Value::string("abc"),
                Value::Int(0),
                Value::Int(4)
            ],
            &mut output
        ),
        Value::Int(3)
    );
    assert_eq!(
        call(
            "strspn",
            vec![Value::string("abc"), Value::string("abc"), Value::Int(4)],
            &mut output
        ),
        Value::Int(0)
    );
    assert_eq!(
        call(
            "strspn",
            vec![Value::string("abc"), Value::string("abc"), Value::Int(-4)],
            &mut output
        ),
        Value::Int(3)
    );
    assert_eq!(
        call(
            "strspn",
            vec![
                Value::string("abc"),
                Value::string("abc"),
                Value::Int(0),
                Value::Int(-4)
            ],
            &mut output
        ),
        Value::Int(0)
    );
    assert_eq!(
        call(
            "strcspn",
            vec![
                Value::string("abc123"),
                Value::string("123"),
                Value::Int(0),
                Value::Int(6)
            ],
            &mut output
        ),
        Value::Int(3)
    );
    assert_eq!(
        call(
            "strcspn",
            vec![Value::string("abc"), Value::string("x"), Value::Int(4)],
            &mut output
        ),
        Value::Int(0)
    );
    assert_eq!(
        call(
            "strcspn",
            vec![Value::string("abc"), Value::string("x"), Value::Int(-4)],
            &mut output
        ),
        Value::Int(3)
    );
    assert_eq!(
        call(
            "strcspn",
            vec![
                Value::string("abc"),
                Value::string("x"),
                Value::Int(0),
                Value::Int(-4)
            ],
            &mut output
        ),
        Value::Int(0)
    );
    assert_eq!(
        call(
            "str_contains",
            vec![Value::string("abc"), Value::string("")],
            &mut output
        ),
        Value::Bool(true)
    );
    assert_eq!(
        call(
            "str_starts_with",
            vec![Value::string("abc"), Value::string("ab")],
            &mut output
        ),
        Value::Bool(true)
    );
    assert_eq!(
        call(
            "str_ends_with",
            vec![Value::string("abc"), Value::string("bc")],
            &mut output
        ),
        Value::Bool(true)
    );
    assert_eq!(
        call(
            "strcmp",
            vec![Value::string("a"), Value::string("b")],
            &mut output
        ),
        Value::Int(-1)
    );
    assert_eq!(
        call(
            "strncmp",
            vec![Value::string("abc"), Value::string("abd"), Value::Int(2)],
            &mut output
        ),
        Value::Int(0)
    );
    assert_eq!(
        call(
            "strcasecmp",
            vec![Value::string("ABC"), Value::string("abc")],
            &mut output
        ),
        Value::Int(0)
    );
    assert_eq!(
        call(
            "strncasecmp",
            vec![Value::string("ABx"), Value::string("aby"), Value::Int(2)],
            &mut output
        ),
        Value::Int(0)
    );
    assert_eq!(
        call(
            "addslashes",
            vec![Value::string(b"a\0b\"c\\d".to_vec())],
            &mut output
        ),
        Value::string(b"a\\0b\\\"c\\\\d".to_vec())
    );
    assert_eq!(
        call(
            "addcslashes",
            vec![Value::string(b"100%_a\\b".to_vec()), Value::string("_%\\")],
            &mut output
        ),
        Value::string(b"100\\%\\_a\\\\b".to_vec())
    );
    assert_eq!(
        call(
            "addcslashes",
            vec![Value::string("Ab-1"), Value::string("A..Za..z")],
            &mut output
        ),
        Value::string("\\A\\b-1")
    );
    assert_eq!(
        call(
            "addcslashes",
            vec![
                Value::string(b"A\0\n\t\x7f".to_vec()),
                Value::string(b"A\0\n\t\x7f".to_vec())
            ],
            &mut output
        ),
        Value::string(b"\\A\\000\\n\\t\\177".to_vec())
    );
    assert_eq!(
        call(
            "stripslashes",
            vec![Value::string(b"a\\0b\\\"c\\\\d".to_vec())],
            &mut output
        ),
        Value::string(b"a\0b\"c\\d".to_vec())
    );
    assert_eq!(
        call(
            "stripcslashes",
            vec![Value::string(br"hello\n\x57\157rld".to_vec())],
            &mut output
        ),
        Value::string(b"hello\nWorld".to_vec())
    );
}

#[test]
fn string_builtins_report_value_errors() {
    for (name, args) in [
        (
            "strpos",
            vec![Value::string("abc"), Value::string("a"), Value::Int(4)],
        ),
        (
            "strncmp",
            vec![Value::string("a"), Value::string("a"), Value::Int(-1)],
        ),
    ] {
        let entry = BuiltinRegistry::new().get(name).expect("builtin exists");
        let mut output = OutputBuffer::new();
        let mut context = BuiltinContext::new(&mut output);
        let error = (entry.function())(&mut context, args, RuntimeSourceSpan::default())
            .expect_err("expected value error");
        assert_eq!(error.diagnostic_id(), "E_PHP_RUNTIME_BUILTIN_VALUE");
    }
}

#[test]
fn strtok_warns_after_delimiter_only_input_needs_new_input() {
    let mut output = OutputBuffer::new();
    let mut state = StrtokState::default();
    let diagnostics = {
        let mut context = BuiltinContext::new(&mut output);
        context.set_strtok_state(&mut state);
        assert_eq!(
            call_in_context(
                &mut context,
                "strtok",
                vec![Value::string(b"\0".to_vec()), Value::string(b"\0".to_vec()),],
            ),
            Value::Bool(false)
        );
        assert_eq!(
            call_in_context(&mut context, "strtok", vec![Value::string(b"\0".to_vec())]),
            Value::Bool(false)
        );
        context.take_diagnostics()
    };

    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].id(), "E_PHP_RUNTIME_STRTOK_MISSING_INPUT");
    assert_eq!(
        diagnostics[0].message(),
        "strtok(): Both arguments must be provided when starting tokenization"
    );
}

#[test]
fn strtok_single_trailing_delimiter_exhausts_without_warning() {
    let mut output = OutputBuffer::new();
    let mut state = StrtokState::default();
    let diagnostics = {
        let mut context = BuiltinContext::new(&mut output);
        context.set_strtok_state(&mut state);
        assert_eq!(
            call_in_context(
                &mut context,
                "strtok",
                vec![
                    Value::string(b"a\0".to_vec()),
                    Value::string(b"\0".to_vec()),
                ],
            ),
            Value::string("a")
        );
        assert_eq!(
            call_in_context(&mut context, "strtok", vec![Value::string(b"\0".to_vec())]),
            Value::Bool(false)
        );
        assert_eq!(
            call_in_context(&mut context, "strtok", vec![Value::string(b"\0".to_vec())]),
            Value::Bool(false)
        );
        context.take_diagnostics()
    };

    assert!(diagnostics.is_empty());
}

#[test]
fn strtok_warns_after_multi_trailing_delimiter_grace_false() {
    let mut output = OutputBuffer::new();
    let mut state = StrtokState::default();
    let diagnostics = {
        let mut context = BuiltinContext::new(&mut output);
        context.set_strtok_state(&mut state);
        assert_eq!(
            call_in_context(
                &mut context,
                "strtok",
                vec![
                    Value::string(b"a\0\0".to_vec()),
                    Value::string(b"\0".to_vec()),
                ],
            ),
            Value::string("a")
        );
        assert_eq!(
            call_in_context(&mut context, "strtok", vec![Value::string(b"\0".to_vec())]),
            Value::Bool(false)
        );
        assert_eq!(
            call_in_context(&mut context, "strtok", vec![Value::string(b"\0".to_vec())]),
            Value::Bool(false)
        );
        context.take_diagnostics()
    };

    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].id(), "E_PHP_RUNTIME_STRTOK_MISSING_INPUT");
}

#[test]
fn strtr_three_string_form_transforms_in_release_builds() {
    let mut output = OutputBuffer::new();
    assert_eq!(
        call(
            "strtr",
            vec![
                Value::string("Requests"),
                Value::string("\\"),
                Value::string("/"),
            ],
            &mut output,
        ),
        Value::string("Requests")
    );
    assert_eq!(
        call(
            "strtr",
            vec![
                Value::string("Namespace\\Class"),
                Value::string("\\"),
                Value::string("/"),
            ],
            &mut output,
        ),
        Value::string("Namespace/Class")
    );
}

#[test]
fn string_split_replace_case_and_padding_builtins_work() {
    let mut output = OutputBuffer::new();

    assert_eq!(
        call(
            "explode",
            vec![Value::string(","), Value::string("a,b,c")],
            &mut output
        ),
        Value::packed_array(vec![
            Value::string("a"),
            Value::string("b"),
            Value::string("c")
        ])
    );
    assert_eq!(
        call(
            "implode",
            vec![
                Value::string("|"),
                Value::packed_array(vec![Value::string("a"), Value::string("b")]),
            ],
            &mut output,
        ),
        Value::string("a|b")
    );
    assert_eq!(
        call(
            "join",
            vec![Value::packed_array(vec![
                Value::string("a"),
                Value::string("b")
            ])],
            &mut output,
        ),
        Value::string("ab")
    );
    assert_eq!(
        call(
            "str_replace",
            vec![
                Value::packed_array(vec![Value::string("a"), Value::string("b")]),
                Value::packed_array(vec![Value::string("x"), Value::string("y")]),
                Value::string("abca"),
            ],
            &mut output,
        ),
        Value::string("xycx")
    );
    assert_eq!(
        call(
            "str_replace",
            vec![
                Value::packed_array(vec![Value::string("-.txt"), Value::string(".txt")]),
                Value::string("-1.txt"),
                Value::string("phrust-admin-core-upload.txt"),
            ],
            &mut output,
        ),
        Value::string("phrust-admin-core-upload-1.txt")
    );
    assert_eq!(
        call(
            "str_replace",
            vec![
                Value::packed_array(vec![Value::string("a"), Value::string("b")]),
                Value::packed_array(vec![Value::string("x")]),
                Value::string("abca"),
            ],
            &mut output,
        ),
        Value::string("xcx")
    );
    assert_eq!(
        call(
            "strtr",
            vec![
                Value::string("abc"),
                Value::string("ab"),
                Value::string("xy")
            ],
            &mut output
        ),
        Value::string("xyc")
    );
    assert_eq!(
        call(
            "strtr",
            vec![
                Value::string("012atm"),
                Value::string("101234567000"),
                Value::string("atm012"),
            ],
            &mut output
        ),
        Value::string("tm0atm")
    );
    assert_eq!(
        call_error(
            "strtr",
            vec![Value::string("012atm"), Value::Int(1)],
            &mut output
        ),
        "strtr(): Argument #2 ($from) must be of type array, int given"
    );
    assert_eq!(
        call_error(
            "strtr",
            vec![
                Value::string("012atm"),
                Value::Array(PhpArray::new()),
                Value::string("atm012"),
            ],
            &mut output
        ),
        "strtr(): Argument #2 ($from) must be of type string, array given"
    );
    assert_eq!(
        call(
            "strtr",
            vec![
                Value::string("012atm"),
                Value::Null,
                Value::string("atm012"),
            ],
            &mut output
        ),
        Value::string("012atm")
    );
    assert!(
        output
            .to_string_lossy()
            .contains("Deprecated: strtr(): Passing null to parameter #2 ($from)")
    );
    assert_eq!(
        call("trim", vec![Value::string(" x ")], &mut output),
        Value::string("x")
    );
    assert_eq!(
        call("ltrim", vec![Value::string(" x ")], &mut output),
        Value::string("x ")
    );
    assert_eq!(
        call("rtrim", vec![Value::string(" x ")], &mut output),
        Value::string(" x")
    );
    assert_eq!(
        call("strtolower", vec![Value::string("AbC")], &mut output),
        Value::string("abc")
    );
    assert_eq!(
        call("strtoupper", vec![Value::string("AbC")], &mut output),
        Value::string("ABC")
    );
    assert_eq!(
        call("strtoupper", vec![Value::Bool(true)], &mut output),
        Value::string("1")
    );
    assert_eq!(
        call("strtoupper", vec![Value::Bool(false)], &mut output),
        Value::string("")
    );
    assert_eq!(
        call("ucfirst", vec![Value::string("abc")], &mut output),
        Value::string("Abc")
    );
    assert_eq!(
        call("lcfirst", vec![Value::string("Abc")], &mut output),
        Value::string("abc")
    );
    assert_eq!(
        call("ucwords", vec![Value::string("a b")], &mut output),
        Value::string("A B")
    );
    assert_eq!(
        call(
            "str_repeat",
            vec![Value::string("ab"), Value::Int(3)],
            &mut output
        ),
        Value::string("ababab")
    );
    assert_eq!(
        call(
            "str_pad",
            vec![
                Value::string("x"),
                Value::Int(3),
                Value::string("0"),
                Value::Int(0)
            ],
            &mut output,
        ),
        Value::string("00x")
    );
    assert_eq!(
        call("strrev", vec![Value::string("abc")], &mut output),
        Value::string("cba")
    );
    assert_eq!(
        call(
            "strnatcasecmp",
            vec![Value::string("pIc 6"), Value::string("pic   7")],
            &mut output
        ),
        Value::Int(-1)
    );
    assert_eq!(
        call(
            "strnatcasecmp",
            vec![Value::string("1.010"), Value::string("1.001")],
            &mut output
        ),
        Value::Int(1)
    );
    assert_eq!(
        call(
            "strnatcmp",
            vec![Value::string("foo   2"), Value::string("foo 10")],
            &mut output
        ),
        Value::Int(-1)
    );
}

#[test]
fn highlight_string_renders_php_style_markup() {
    let mut output = OutputBuffer::new();
    assert_eq!(
        call(
            "highlight_string",
            vec![Value::string("<br /><?php echo \"foo\"; ?><br />")],
            &mut output
        ),
        Value::Bool(true)
    );
    assert_eq!(
        output.to_string_lossy(),
        "<pre><code style=\"color: #000000\">&lt;br /&gt;<span style=\"color: #0000BB\">&lt;?php </span><span style=\"color: #007700\">echo </span><span style=\"color: #DD0000\">\"foo\"</span><span style=\"color: #007700\">; </span><span style=\"color: #0000BB\">?&gt;</span>&lt;br /&gt;</code></pre>"
    );

    assert_eq!(
        call(
            "highlight_string",
            vec![
                Value::string("<?php echo \"foo[] $a \\n\"; ?>"),
                Value::Bool(true)
            ],
            &mut output
        ),
        Value::string(
            "<pre><code style=\"color: #000000\"><span style=\"color: #0000BB\">&lt;?php </span><span style=\"color: #007700\">echo </span><span style=\"color: #DD0000\">\"foo[] </span><span style=\"color: #0000BB\">$a</span><span style=\"color: #DD0000\"> \\n\"</span><span style=\"color: #007700\">; </span><span style=\"color: #0000BB\">?&gt;</span></code></pre>"
        )
    );
}

#[test]
fn string_split_replace_reports_value_errors() {
    for (name, args) in [
        ("explode", vec![Value::string(""), Value::string("abc")]),
        ("str_repeat", vec![Value::string("x"), Value::Int(-1)]),
        (
            "str_pad",
            vec![Value::string("x"), Value::Int(3), Value::string("")],
        ),
    ] {
        let entry = BuiltinRegistry::new().get(name).expect("builtin exists");
        let mut output = OutputBuffer::new();
        let mut context = BuiltinContext::new(&mut output);
        let error = (entry.function())(&mut context, args, RuntimeSourceSpan::default())
            .expect_err("expected value error");
        assert_eq!(error.diagnostic_id(), "E_PHP_RUNTIME_BUILTIN_VALUE");
    }
}

#[test]
fn encoding_hash_html_and_url_builtins_cover_mvp_paths() {
    let mut output = OutputBuffer::new();
    fn array_value(array: &PhpArray, key: &str) -> Option<Value> {
        array
            .get(&ArrayKey::String(PhpString::from_test_str(key)))
            .cloned()
    }

    assert_eq!(
        call("bin2hex", vec![Value::string("Hi")], &mut output),
        Value::string("4869")
    );
    assert_eq!(
        call("hex2bin", vec![Value::string("4869")], &mut output),
        Value::string("Hi")
    );
    assert_eq!(
        call("hex2bin", vec![Value::string("f")], &mut output),
        Value::Bool(false)
    );
    assert_eq!(
        call("hex2bin", vec![Value::string("zz")], &mut output),
        Value::Bool(false)
    );
    assert_eq!(
        call("ord", vec![Value::string("A")], &mut output),
        Value::Int(65)
    );
    assert_eq!(
        call("chr", vec![Value::Int(321)], &mut output),
        Value::string("A")
    );
    assert_eq!(
        call("md5", vec![Value::string("abc")], &mut output),
        Value::string("900150983cd24fb0d6963f7d28e17f72")
    );
    assert_eq!(
        call("sha1", vec![Value::string("abc")], &mut output),
        Value::string("a9993e364706816aba3e25717850c26c9cd0d89d")
    );
    assert_eq!(
        call("crc32", vec![Value::string("abc")], &mut output),
        Value::Int(891_568_578)
    );
    assert_eq!(
        call("base64_encode", vec![Value::string("hi")], &mut output),
        Value::string("aGk=")
    );
    assert_eq!(
        call("base64_decode", vec![Value::string("aGk=")], &mut output),
        Value::string("hi")
    );
    assert_eq!(
        call(
            "base64_decode",
            vec![Value::string("a!Gk="), Value::Bool(false)],
            &mut output
        ),
        Value::string("hi")
    );
    assert_eq!(
        call(
            "base64_decode",
            vec![Value::string("a!Gk="), Value::Bool(true)],
            &mut output
        ),
        Value::Bool(false)
    );
    assert_eq!(
        call(
            "htmlspecialchars",
            vec![Value::string("<a&\"'>")],
            &mut output
        ),
        Value::string("&lt;a&amp;&quot;&#039;&gt;")
    );
    assert_eq!(
        call(
            "htmlspecialchars",
            vec![
                Value::string("?a=1&amp;b=2&#038;c=3&#x26;d=4"),
                Value::Int(3),
                Value::string("UTF-8"),
                Value::Bool(false)
            ],
            &mut output
        ),
        Value::string("?a=1&amp;b=2&#038;c=3&#x26;d=4")
    );
    assert_eq!(
        call(
            "htmlspecialchars",
            vec![
                Value::string("PhrustBenchmark &raquo; Feed"),
                Value::Int(3),
                Value::string("UTF-8"),
                Value::Bool(false)
            ],
            &mut output
        ),
        Value::string("PhrustBenchmark &raquo; Feed")
    );
    assert_eq!(
        call(
            "htmlspecialchars",
            vec![
                Value::string("&bogus;"),
                Value::Int(3),
                Value::string("UTF-8"),
                Value::Bool(false)
            ],
            &mut output
        ),
        Value::string("&amp;bogus;")
    );
    assert_eq!(
        call(
            "htmlspecialchars",
            vec![
                Value::string("\"'"),
                Value::Int(0),
                Value::string("UTF-8"),
                Value::Bool(true)
            ],
            &mut output
        ),
        Value::string("\"'")
    );
    assert_eq!(
        call(
            "htmlspecialchars",
            vec![
                Value::string("\"'"),
                Value::Int(2),
                Value::string("UTF-8"),
                Value::Bool(true)
            ],
            &mut output
        ),
        Value::string("&quot;'")
    );
    assert_eq!(
        call(
            "htmlspecialchars_decode",
            vec![Value::string("&lt;a&amp;&quot;&#039;&gt;")],
            &mut output
        ),
        Value::string("<a&\"'>")
    );
    assert_eq!(
        call(
            "htmlspecialchars_decode",
            vec![
                Value::string("Roy&#039;s &quot;quote&quot; &lt;tag&gt; &amp;"),
                Value::Int(2)
            ],
            &mut output
        ),
        Value::string("Roy&#039;s \"quote\" <tag> &")
    );
    assert_eq!(
        call(
            "htmlspecialchars_decode",
            vec![
                Value::string("Roy&#039;s &quot;quote&quot; &lt;tag&gt; &amp;"),
                Value::Int(0)
            ],
            &mut output
        ),
        Value::string("Roy&#039;s &quot;quote&quot; <tag> &")
    );
    assert_eq!(
        call(
            "htmlspecialchars_decode",
            vec![
                Value::string("&#x22;|&#34;|&#39;|&#x26;|&#60;|&#x3E;|&#63;"),
                Value::Int(3 | 48)
            ],
            &mut output
        ),
        Value::string("\"|\"|'|&|<|>|&#63;")
    );
    assert_eq!(
        call(
            "htmlspecialchars_decode",
            vec![Value::string("&apos;|&#39;"), Value::Int(3)],
            &mut output
        ),
        Value::string("&apos;|'")
    );
    assert_eq!(
        call(
            "htmlspecialchars_decode",
            vec![Value::string("&apos;|&#39;"), Value::Int(3 | 16)],
            &mut output
        ),
        Value::string("'|'")
    );
    assert_eq!(
        call(
            "html_entity_decode",
            vec![
                Value::string("&lt;a&amp;&quot;&#039;&gt;"),
                Value::Int(3),
                Value::string("UTF-8")
            ],
            &mut output
        ),
        Value::string("<a&\"'>")
    );
    assert_eq!(
        call(
            "html_entity_decode",
            vec![
                Value::string("&apos;"),
                Value::Int(3),
                Value::string("UTF-8")
            ],
            &mut output
        ),
        Value::string("&apos;")
    );
    assert_eq!(
        call(
            "html_entity_decode",
            vec![
                Value::string("&apos;"),
                Value::Int(3 | 48),
                Value::string("UTF-8")
            ],
            &mut output
        ),
        Value::string("'")
    );
    assert_eq!(
        call(
            "html_entity_decode",
            vec![
                Value::string("&#x09;|&#x0B;|&#x0D;|&#xD800;"),
                Value::Int(3),
                Value::string("UTF-8")
            ],
            &mut output
        ),
        Value::string("\t|&#x0B;|\r|&#xD800;")
    );
    assert_eq!(
        call(
            "html_entity_decode",
            vec![
                Value::string("&#x0C;|&#x0D;|&#xFDD0;|&#x2FFFF;"),
                Value::Int(3 | 48),
                Value::string("UTF-8")
            ],
            &mut output
        ),
        Value::string("\x0c|&#x0D;|&#xFDD0;|&#x2FFFF;")
    );
    let Value::Array(compat_table) = call(
        "get_html_translation_table",
        vec![Value::Int(0), Value::Int(2), Value::string("UTF-8")],
        &mut output,
    ) else {
        panic!("get_html_translation_table should return an array");
    };
    assert_eq!(compat_table.len(), 4);
    assert_eq!(
        array_value(&compat_table, "&"),
        Some(Value::string("&amp;"))
    );
    assert_eq!(
        array_value(&compat_table, "\""),
        Some(Value::string("&quot;"))
    );
    assert_eq!(array_value(&compat_table, "'"), None);
    let Value::Array(quotes_table) = call(
        "get_html_translation_table",
        vec![Value::Int(0), Value::Int(3), Value::string("UTF-8")],
        &mut output,
    ) else {
        panic!("get_html_translation_table should return an array");
    };
    assert_eq!(quotes_table.len(), 5);
    assert_eq!(
        array_value(&quotes_table, "'"),
        Some(Value::string("&#039;"))
    );
    let Value::Array(xml_table) = call(
        "get_html_translation_table",
        vec![Value::Int(1), Value::Int(3 | 16), Value::string("UTF-8")],
        &mut output,
    ) else {
        panic!("get_html_translation_table should return an array");
    };
    assert_eq!(xml_table.len(), 5);
    assert_eq!(array_value(&xml_table, "'"), Some(Value::string("&apos;")));
    let Value::Array(html5_sjis_table) = call(
        "get_html_translation_table",
        vec![Value::Int(1), Value::Int(3 | 48), Value::string("SJIS")],
        &mut output,
    ) else {
        panic!("get_html_translation_table should return an array");
    };
    assert_eq!(html5_sjis_table.len(), 5);
    assert_eq!(
        array_value(&html5_sjis_table, "\""),
        Some(Value::string("&quot;"))
    );
    assert_eq!(
        call("htmlentities", vec![Value::string("<a&>")], &mut output),
        Value::string("&lt;a&amp;&gt;")
    );
    assert_eq!(
        call(
            "htmlentities",
            vec![
                Value::string("€ © é"),
                Value::Int(0),
                Value::string("UTF-8")
            ],
            &mut output
        ),
        Value::string("&euro; &copy; &eacute;")
    );
    assert_eq!(
        call("urlencode", vec![Value::string("a b~")], &mut output),
        Value::string("a+b%7E")
    );
    assert_eq!(
        call("rawurlencode", vec![Value::string("a b~")], &mut output),
        Value::string("a%20b~")
    );
    assert_eq!(
        call("urldecode", vec![Value::string("a+b%7E")], &mut output),
        Value::string("a b~")
    );
    assert_eq!(
        call("rawurldecode", vec![Value::string("a%20b~")], &mut output),
        Value::string("a b~")
    );

    let mut query = PhpArray::new();
    query.insert(
        ArrayKey::String(PhpString::from_test_str("a")),
        Value::string("b"),
    );
    query.insert(
        ArrayKey::String(PhpString::from_test_str("c")),
        Value::Int(1),
    );
    assert_eq!(
        call("http_build_query", vec![Value::Array(query)], &mut output),
        Value::string("a=b&c=1")
    );
    let mut prefixed_query = PhpArray::new();
    prefixed_query.insert(
        ArrayKey::String(PhpString::from_test_str("foo")),
        Value::string("bar"),
    );
    prefixed_query.insert(ArrayKey::Int(0), Value::string("abc"));
    prefixed_query.insert(
        ArrayKey::String(PhpString::from_test_str("true")),
        Value::Bool(true),
    );
    assert_eq!(
        call(
            "http_build_query",
            vec![
                Value::Array(prefixed_query),
                Value::string("num"),
                Value::string(";")
            ],
            &mut output
        ),
        Value::string("foo=bar;num0=abc;true=1")
    );
    let mut raw_query = PhpArray::new();
    raw_query.insert(
        ArrayKey::String(PhpString::from_test_str("a b")),
        Value::string("c d"),
    );
    assert_eq!(
        call(
            "http_build_query",
            vec![
                Value::Array(raw_query),
                Value::string(""),
                Value::Null,
                Value::Int(PHP_QUERY_RFC3986)
            ],
            &mut output
        ),
        Value::string("a%20b=c%20d")
    );

    let mut resource_table = ResourceTable::new();
    let resource = resource_table.register_stdin(Vec::new());
    let mut resource_query = PhpArray::new();
    resource_query.insert(ArrayKey::Int(0), Value::Resource(resource));
    resource_query.insert(ArrayKey::Int(1), Value::string("kept"));
    assert_eq!(
        call(
            "http_build_query",
            vec![Value::Array(resource_query)],
            &mut output
        ),
        Value::string("1=kept")
    );

    let entry = BuiltinRegistry::new()
        .get("http_build_query")
        .expect("builtin exists");
    let mut separator_ini = crate::IniRegistry::default();
    assert_eq!(
        separator_ini.set("arg_separator.output", ";"),
        Some("&".to_owned())
    );
    let mut context = BuiltinContext::new(&mut output);
    context.set_ini_registry(separator_ini);
    let mut separated_query = PhpArray::new();
    separated_query.insert(
        ArrayKey::String(PhpString::from_test_str("a")),
        Value::string("b"),
    );
    separated_query.insert(
        ArrayKey::String(PhpString::from_test_str("c")),
        Value::string("d"),
    );
    assert_eq!(
        (entry.function())(
            &mut context,
            vec![Value::Array(separated_query)],
            RuntimeSourceSpan::default()
        )
        .expect("builtin ok"),
        Value::string("a=b;c=d")
    );
}

#[test]
fn strip_tags_uses_php_tag_state_machine() {
    let mut output = OutputBuffer::new();

    assert_eq!(
        call(
            "strip_tags",
            vec![Value::string("NEAT <? cool < blah ?> STUFF")],
            &mut output
        ),
        Value::string("NEAT  STUFF")
    );
    assert_eq!(
        call(
            "strip_tags",
            vec![Value::string("NEAT <!-- cool > blah --> STUFF")],
            &mut output
        ),
        Value::string("NEAT  STUFF")
    );
    assert_eq!(
        call(
            "strip_tags",
            vec![Value::string("hello <img title=\">_<\"> world")],
            &mut output
        ),
        Value::string("hello  world")
    );
    assert_eq!(
        call(
            "strip_tags",
            vec![Value::string("<html> I am html string </html>\0<?php x ?>")],
            &mut output
        ),
        Value::string(" I am html string ")
    );
}

#[test]
fn strip_tags_normalizes_allowed_tags_like_php() {
    let mut output = OutputBuffer::new();

    assert_eq!(
        call(
            "strip_tags",
            vec![
                Value::string("<<htmL>>hello<</htmL>>"),
                Value::string("<<html>>")
            ],
            &mut output
        ),
        Value::string("<htmL>hello</htmL>")
    );

    let mut allowed = PhpArray::new();
    allowed.append(Value::string("html"));
    assert_eq!(
        call(
            "strip_tags",
            vec![
                Value::string("<html>hello</html><p>world</p>"),
                Value::Array(allowed)
            ],
            &mut output
        ),
        Value::string("<html>hello</html>world")
    );

    let error = call_error(
        "strip_tags",
        vec![
            Value::string("<html>hello</html>"),
            Value::Resource(ResourceTable::new().register_stream(
                StreamFlags::new(true, false, false),
                StreamMetadata::new("php", "stream", "r", "memory"),
            )),
        ],
        &mut output,
    );
    assert_eq!(
        error,
        "strip_tags(): Argument #2 ($allowed_tags) must be of type array|string|null, resource given"
    );
}

#[test]
fn parse_url_covers_standard_strings_module_cases() {
    let mut output = OutputBuffer::new();

    let empty = call("parse_url", vec![Value::string("")], &mut output);
    let Value::Array(empty_parts) = empty else {
        panic!("parse_url should return an array for an empty URL");
    };
    assert_eq!(
        empty_parts.get(&ArrayKey::String(PhpString::from_test_str("path"))),
        Some(&Value::string(""))
    );

    let host_port = call(
        "parse_url",
        vec![Value::string("64.246.30.37:80/")],
        &mut output,
    );
    let Value::Array(host_port_parts) = host_port else {
        panic!("parse_url should return host and port parts");
    };
    assert_eq!(
        host_port_parts.get(&ArrayKey::String(PhpString::from_test_str("host"))),
        Some(&Value::string("64.246.30.37"))
    );
    assert_eq!(
        host_port_parts.get(&ArrayKey::String(PhpString::from_test_str("port"))),
        Some(&Value::Int(80))
    );
    assert_eq!(
        host_port_parts.get(&ArrayKey::String(PhpString::from_test_str("path"))),
        Some(&Value::string("/"))
    );

    let full = Value::string("http://secret:hideout@www.php.net:80/index.php?test=1#frag");
    let full_parts = call("parse_url", vec![full.clone()], &mut output);
    let Value::Array(full_parts) = full_parts else {
        panic!("parse_url should return full URL parts");
    };
    assert_eq!(
        full_parts.get(&ArrayKey::String(PhpString::from_test_str("scheme"))),
        Some(&Value::string("http"))
    );
    assert_eq!(
        full_parts.get(&ArrayKey::String(PhpString::from_test_str("user"))),
        Some(&Value::string("secret"))
    );
    assert_eq!(
        full_parts.get(&ArrayKey::String(PhpString::from_test_str("pass"))),
        Some(&Value::string("hideout"))
    );
    assert_eq!(
        full_parts.get(&ArrayKey::String(PhpString::from_test_str("query"))),
        Some(&Value::string("test=1"))
    );
    assert_eq!(
        call("parse_url", vec![full.clone(), Value::Int(0)], &mut output),
        Value::string("http")
    );
    assert_eq!(
        call("parse_url", vec![full.clone(), Value::Int(2)], &mut output),
        Value::Int(80)
    );
    assert_eq!(
        call("parse_url", vec![full.clone(), Value::Int(-1)], &mut output),
        Value::Array(full_parts.clone())
    );
    assert_eq!(
        call_error("parse_url", vec![full.clone(), Value::Int(99)], &mut output),
        "parse_url(): Argument #2 ($component) must be a valid URL component identifier, 99 given"
    );
    assert_eq!(
        call(
            "parse_url",
            vec![Value::string("http://1.2.3.4:/abc.asp?a=1&b=2")],
            &mut output,
        ),
        {
            let mut expected = PhpArray::new();
            expected.insert(
                ArrayKey::String(PhpString::from_test_str("scheme")),
                Value::string("http"),
            );
            expected.insert(
                ArrayKey::String(PhpString::from_test_str("host")),
                Value::string("1.2.3.4"),
            );
            expected.insert(
                ArrayKey::String(PhpString::from_test_str("path")),
                Value::string("/abc.asp"),
            );
            expected.insert(
                ArrayKey::String(PhpString::from_test_str("query")),
                Value::string("a=1&b=2"),
            );
            Value::Array(expected)
        }
    );
    assert_eq!(
        call("parse_url", vec![Value::string("x://::6.5")], &mut output,),
        {
            let mut expected = PhpArray::new();
            expected.insert(
                ArrayKey::String(PhpString::from_test_str("scheme")),
                Value::string("x"),
            );
            expected.insert(
                ArrayKey::String(PhpString::from_test_str("host")),
                Value::string(":"),
            );
            expected.insert(
                ArrayKey::String(PhpString::from_test_str("port")),
                Value::Int(6),
            );
            Value::Array(expected)
        }
    );
    assert_eq!(
        call(
            "parse_url",
            vec![Value::string("http://example.com:80abc/path")],
            &mut output,
        ),
        {
            let mut expected = PhpArray::new();
            expected.insert(
                ArrayKey::String(PhpString::from_test_str("scheme")),
                Value::string("http"),
            );
            expected.insert(
                ArrayKey::String(PhpString::from_test_str("host")),
                Value::string("example.com"),
            );
            expected.insert(
                ArrayKey::String(PhpString::from_test_str("port")),
                Value::Int(80),
            );
            expected.insert(
                ArrayKey::String(PhpString::from_test_str("path")),
                Value::string("/path"),
            );
            Value::Array(expected)
        }
    );
    assert_eq!(
        call("parse_url", vec![Value::string("x://::abc/?")], &mut output,),
        Value::Bool(false)
    );
}

#[test]
fn native_url_query_parsers_preserve_typed_structure() {
    use crate::api::{
        NativeInputKey as InputKey, NativeInputSegment as InputSegment,
        NativeStructuredValuePublisher, decode_native_json_associative_into,
    };
    use crate::builtins::JsonRequestState;

    #[derive(Debug, PartialEq)]
    enum Native {
        Null,
        Bool(bool),
        Int(i64),
        Float(f64),
        String(Vec<u8>),
        Array(Vec<Self>),
        Object(Vec<(Vec<u8>, Self)>),
    }

    struct TestPublisher;

    impl NativeStructuredValuePublisher for TestPublisher {
        type Output = Native;

        fn publish_null(&mut self) -> Option<Self::Output> {
            Some(Native::Null)
        }

        fn publish_bool(&mut self, value: bool) -> Option<Self::Output> {
            Some(Native::Bool(value))
        }

        fn publish_int(&mut self, value: i64) -> Option<Self::Output> {
            Some(Native::Int(value))
        }

        fn publish_float(&mut self, value: f64) -> Option<Self::Output> {
            Some(Native::Float(value))
        }

        fn publish_string(&mut self, value: &[u8]) -> Option<Self::Output> {
            Some(Native::String(value.to_vec()))
        }

        fn rollback(&mut self, _value: Self::Output) {}

        fn publish_array_stream<E>(
            &mut self,
            build: impl FnOnce(
                &mut Self,
                &mut dyn FnMut(&mut Self, Self::Output) -> Option<()>,
            ) -> Result<(), E>,
        ) -> Result<Option<Self::Output>, E> {
            let mut values: Vec<Native> = Vec::new();
            {
                let mut push = |_: &mut Self, value| {
                    values.push(value);
                    Some(())
                };
                build(self, &mut push)?;
            }
            Ok(Some(Native::Array(values)))
        }

        fn publish_object_stream<E>(
            &mut self,
            build: impl FnOnce(
                &mut Self,
                &mut dyn FnMut(&mut Self, &[u8], Self::Output) -> Option<()>,
            ) -> Result<(), E>,
        ) -> Result<Option<Self::Output>, E> {
            let mut values: Vec<(Vec<u8>, Native)> = Vec::new();
            {
                let mut push = |_: &mut Self, key: &[u8], value| {
                    if let Some((_, previous)) = values
                        .iter_mut()
                        .find(|(existing, _)| existing.as_slice() == key)
                    {
                        *previous = value;
                    } else {
                        values.push((key.to_vec(), value));
                    }
                    Some(())
                };
                build(self, &mut push)?;
            }
            Ok(Some(Native::Object(values)))
        }

        fn publish_array_with(
            &mut self,
            length: usize,
            mut build: impl FnMut(&mut Self, usize) -> Option<Self::Output>,
        ) -> Option<Self::Output> {
            let values = (0..length)
                .map(|index| build(self, index))
                .collect::<Option<Vec<_>>>()?;
            Some(Native::Array(values))
        }
    }

    let mut publisher = TestPublisher;

    assert_eq!(
        crate::api::native_parse_url_into(
            b"http://user:pass@example.com:8080/a?x=1#fragment",
            None,
            &mut publisher,
        ),
        Ok((
            true,
            Some(Native::Object(vec![
                (b"scheme".to_vec(), Native::String(b"http".to_vec())),
                (b"host".to_vec(), Native::String(b"example.com".to_vec())),
                (b"port".to_vec(), Native::Int(8080)),
                (b"user".to_vec(), Native::String(b"user".to_vec())),
                (b"pass".to_vec(), Native::String(b"pass".to_vec())),
                (b"path".to_vec(), Native::String(b"/a".to_vec())),
                (b"query".to_vec(), Native::String(b"x=1".to_vec())),
                (b"fragment".to_vec(), Native::String(b"fragment".to_vec()),),
            ]))
        ))
    );
    assert_eq!(
        crate::api::native_parse_url_into(b"http://example.com/a", Some(1), &mut publisher),
        Ok((true, Some(Native::String(b"example.com".to_vec()))))
    );
    assert_eq!(
        crate::api::native_parse_url_into(b"x://::abc/?", None, &mut publisher),
        Ok((false, None))
    );
    assert_eq!(
        crate::api::native_parse_url_into(b"http://example.com", Some(99), &mut publisher),
        Err(99)
    );

    let mut json_state = JsonRequestState::default();
    assert_eq!(
        decode_native_json_associative_into(
            &mut json_state,
            br#"{"a":[1,{"b":"x"}],"a":[2],"empty":{},"big":18446744073709551616}"#,
            8,
            &mut publisher,
        )
        .expect("native JSON streaming decode succeeds"),
        Some(Native::Object(vec![
            (b"a".to_vec(), Native::Array(vec![Native::Int(2)])),
            (b"empty".to_vec(), Native::Object(vec![])),
            (
                b"big".to_vec(),
                Native::Float(18_446_744_073_709_551_616_u128 as f64),
            ),
        ]))
    );
    assert_eq!(
        json_state.value().0,
        crate::builtins::context::JSON_ERROR_NONE
    );
    assert_eq!(
        decode_native_json_associative_into(&mut json_state, b"[[]]", 1, &mut publisher)
            .expect("native JSON depth failure remains a PHP result"),
        Some(Native::Null)
    );
    assert_eq!(
        json_state.value().0,
        crate::builtins::context::JSON_ERROR_DEPTH
    );

    let ini = RuntimeIniOptions::default();
    let mut insertions = Vec::new();
    assert_eq!(
        crate::api::native_parse_str_into(
            b"plain=first&plain=last&list[]=a&list[]=b&nested[x]=y&12=numeric",
            &ini,
            |segments, value| {
                insertions.push((segments.to_vec(), value.to_vec()));
                Ok::<_, ()>(())
            },
        ),
        Ok(())
    );
    assert_eq!(
        insertions,
        vec![
            (
                vec![InputSegment::Key(InputKey::String(b"plain".to_vec()))],
                b"first".to_vec(),
            ),
            (
                vec![InputSegment::Key(InputKey::String(b"plain".to_vec()))],
                b"last".to_vec(),
            ),
            (
                vec![
                    InputSegment::Key(InputKey::String(b"list".to_vec())),
                    InputSegment::Append,
                ],
                b"a".to_vec(),
            ),
            (
                vec![
                    InputSegment::Key(InputKey::String(b"list".to_vec())),
                    InputSegment::Append,
                ],
                b"b".to_vec(),
            ),
            (
                vec![
                    InputSegment::Key(InputKey::String(b"nested".to_vec())),
                    InputSegment::Key(InputKey::String(b"x".to_vec())),
                ],
                b"y".to_vec(),
            ),
            (
                vec![InputSegment::Key(InputKey::Int(12))],
                b"numeric".to_vec(),
            ),
        ]
    );
}

#[test]
fn substr_treats_null_length_like_omitted_length() {
    let mut output = OutputBuffer::new();

    assert_eq!(
        call(
            "substr",
            vec![Value::string("abcdef"), Value::Int(2), Value::Null],
            &mut output,
        ),
        Value::string("cdef")
    );
}

#[test]
fn encoding_builtins_report_value_errors() {
    let entry = BuiltinRegistry::new().get("ord").expect("builtin exists");
    let mut output = OutputBuffer::new();
    let mut context = BuiltinContext::new(&mut output);
    let error = (entry.function())(
        &mut context,
        vec![Value::string("")],
        RuntimeSourceSpan::default(),
    )
    .expect_err("expected value error");
    assert_eq!(error.diagnostic_id(), "E_PHP_RUNTIME_BUILTIN_VALUE");
}

#[test]
fn pack_unpack_cover_standard_integer_formats_and_cursor_ops() {
    let mut output = OutputBuffer::new();

    let packed = call(
        "pack",
        vec![
            Value::string("ll"),
            Value::Int(0x0102_0304),
            Value::Int(0x0506_0708),
        ],
        &mut output,
    );
    assert_eq!(
        packed,
        Value::string(vec![0x04, 0x03, 0x02, 0x01, 0x08, 0x07, 0x06, 0x05])
    );

    let mut expected_numeric = PhpArray::new();
    expected_numeric.insert(ArrayKey::Int(1), Value::Int(0x0102_0304));
    expected_numeric.insert(ArrayKey::Int(2), Value::Int(0x0506_0708));
    assert_eq!(
        call(
            "unpack",
            vec![
                Value::string("l2"),
                Value::string(vec![
                    b'p', b'a', b'd', 0x04, 0x03, 0x02, 0x01, 0x08, 0x07, 0x06, 0x05
                ]),
                Value::Int(3),
            ],
            &mut output,
        ),
        Value::Array(expected_numeric)
    );

    let mut expected_named = PhpArray::new();
    expected_named.insert(
        ArrayKey::String(PhpString::from_test_str("a")),
        Value::Int(1),
    );
    expected_named.insert(
        ArrayKey::String(PhpString::from_test_str("b")),
        Value::Int(1),
    );
    expected_named.insert(
        ArrayKey::String(PhpString::from_test_str("c")),
        Value::Int(2),
    );
    expected_named.insert(
        ArrayKey::String(PhpString::from_test_str("d")),
        Value::Int(2),
    );
    let packed_unsigned = call(
        "pack",
        vec![Value::string("VV"), Value::Int(1), Value::Int(2)],
        &mut output,
    );
    assert_eq!(
        call(
            "unpack",
            vec![Value::string("V1a/X4/V1b/V1c/X4/V1d"), packed_unsigned],
            &mut output,
        ),
        Value::Array(expected_named)
    );

    assert_eq!(
        call(
            "pack",
            vec![Value::string("H*"), Value::string("0061f")],
            &mut output,
        ),
        Value::string(vec![0x00, 0x61, 0xf0])
    );
    assert_eq!(
        call(
            "pack",
            vec![Value::string("h*"), Value::string("0061f")],
            &mut output,
        ),
        Value::string(vec![0x00, 0x16, 0x0f])
    );
    assert_eq!(
        call(
            "pack",
            vec![Value::string("H3"), Value::string("0061f")],
            &mut output,
        ),
        Value::string(vec![0x00, 0x60])
    );

    let mut expected_hex = PhpArray::new();
    expected_hex.insert(
        ArrayKey::String(PhpString::from_test_str("a")),
        Value::string("012"),
    );
    expected_hex.insert(
        ArrayKey::String(PhpString::from_test_str("b")),
        Value::string("45"),
    );
    assert_eq!(
        call(
            "unpack",
            vec![
                Value::string("H3a/H2b"),
                Value::string(vec![0x01, 0x23, 0x45]),
            ],
            &mut output,
        ),
        Value::Array(expected_hex)
    );
}

#[test]
fn formatting_builtins_cover_common_printf_surface() {
    let mut output = OutputBuffer::new();

    assert_eq!(
        call(
            "sprintf",
            vec![
                Value::string("%04d|%-5s|%.2f|%08x|%X|%o|%c|%%"),
                Value::Int(7),
                Value::string("x"),
                Value::float(1.25),
                Value::Int(255),
                Value::Int(255),
                Value::Int(8),
                Value::Int(65),
            ],
            &mut output,
        ),
        Value::string("0007|x    |1.25|000000ff|FF|10|A|%")
    );
    assert_eq!(
        call(
            "sprintf",
            vec![
                Value::string("%'_5s|%+d|% d"),
                Value::string("x"),
                Value::Int(7),
                Value::Int(7)
            ],
            &mut output,
        ),
        Value::string("____x|+7|7")
    );
    assert_eq!(
        call(
            "sprintf",
            vec![Value::string("%-010.2f"), Value::float(2.5)],
            &mut output,
        ),
        Value::string("2.50000000")
    );
    assert_eq!(
        call(
            "sprintf",
            vec![
                Value::string("%3$s %1$s %2$04d %4$'#5s %5$ls"),
                Value::string("one"),
                Value::Int(2),
                Value::string("three"),
                Value::string("x"),
                Value::string("wide"),
            ],
            &mut output,
        ),
        Value::string("three one 0002 ####x wide")
    );
    assert_eq!(
        call(
            "sprintf",
            vec![Value::string("% %%d"), Value::Int(1234), Value::Int(-5678)],
            &mut output,
        ),
        Value::string("%-5678")
    );
    assert_eq!(
        call(
            "sprintf",
            vec![
                Value::string("%b|%e|%E|%g|%G|%.3g|%.3G"),
                Value::Int(-5),
                Value::Int(1000),
                Value::Int(1000),
                Value::float(1.25),
                Value::float(0.0000123),
                Value::Int(1000),
                Value::float(1234567.0)
            ],
            &mut output,
        ),
        Value::string(
            "1111111111111111111111111111111111111111111111111111111111111011|1.000000e+3|1.000000E+3|1.25|1.23E-5|1.0e+3|1.23E+6"
        )
    );
    assert_eq!(
        call(
            "sprintf",
            vec![
                Value::string("%.4d|%04.4u|%10.4o|%-10.4x|%04.4b"),
                Value::Int(123),
                Value::Int(123),
                Value::Int(123),
                Value::Int(123),
                Value::Int(123)
            ],
            &mut output,
        ),
        Value::string("123|0123|          |          |0000")
    );

    assert_eq!(
        call(
            "printf",
            vec![Value::string("[%04d]"), Value::Int(7)],
            &mut output
        ),
        Value::Int(6)
    );
    assert_eq!(output.to_string_lossy(), "[0007]");

    let args = Value::packed_array(vec![Value::string("id"), Value::Int(9)]);
    assert_eq!(
        call(
            "vsprintf",
            vec![Value::string("%s:%d"), args.clone()],
            &mut output,
        ),
        Value::string("id:9")
    );
    assert_eq!(
        call("vprintf", vec![Value::string("%s:%d"), args], &mut output),
        Value::Int(4)
    );
    assert_eq!(output.to_string_lossy(), "[0007]id:9");
}

#[test]
fn formatting_builtins_report_missing_args_and_stream_writes() {
    for (name, args, expected_id) in [
        (
            "sprintf",
            vec![Value::string("%s %s"), Value::string("only-one")],
            "E_PHP_RUNTIME_PRINTF_ARGUMENTS",
        ),
        (
            "fprintf",
            vec![Value::Null, Value::string("%s"), Value::string("x")],
            "E_PHP_RUNTIME_BUILTIN_TYPE",
        ),
    ] {
        let entry = BuiltinRegistry::new().get(name).expect("builtin exists");
        let mut output = OutputBuffer::new();
        let mut context = BuiltinContext::new(&mut output);
        let error = (entry.function())(&mut context, args, RuntimeSourceSpan::default())
            .expect_err("expected formatting error");
        assert_eq!(error.diagnostic_id(), expected_id);
    }

    let entry = BuiltinRegistry::new()
        .get("vfprintf")
        .expect("builtin exists");
    let mut output = OutputBuffer::new();
    let mut context = BuiltinContext::new(&mut output);
    let error = (entry.function())(
        &mut context,
        vec![
            Value::string("stream"),
            Value::string("%s"),
            Value::Array(PhpArray::new()),
        ],
        RuntimeSourceSpan::default(),
    )
    .expect_err("expected stream type error");
    assert_eq!(
        error.message(),
        "vfprintf(): Argument #1 ($stream) must be of type resource, string given"
    );
    let error = (entry.function())(
        &mut context,
        vec![
            Value::Resource(ResourceTable::new().register_stream(
                StreamFlags::new(true, true, true),
                StreamMetadata::new("php", "stream", "w+", "php://memory"),
            )),
            Value::Array(PhpArray::new()),
            Value::Array(PhpArray::new()),
        ],
        RuntimeSourceSpan::default(),
    )
    .expect_err("expected format type error");
    assert_eq!(
        error.message(),
        "vfprintf(): Argument #2 ($format) must be of type string, array given"
    );
    let error = (entry.function())(
        &mut context,
        vec![
            Value::Resource(ResourceTable::new().register_stream(
                StreamFlags::new(true, true, true),
                StreamMetadata::new("php", "stream", "w+", "php://memory"),
            )),
            Value::string("%s"),
            Value::Null,
        ],
        RuntimeSourceSpan::default(),
    )
    .expect_err("expected values type error");
    assert_eq!(
        error.message(),
        "vfprintf(): Argument #3 ($values) must be of type array, null given"
    );
    assert_eq!(
        call_error(
            "vfprintf",
            vec![
                Value::Resource(ResourceTable::new().register_stream(
                    StreamFlags::new(true, true, true),
                    StreamMetadata::new("php", "stream", "w+", "php://memory"),
                )),
                Value::string("Foo %y fake"),
                Value::packed_array(vec![Value::string("x")]),
            ],
            &mut output,
        ),
        "Unknown format specifier \"y\""
    );
    assert_eq!(
        call_error(
            "vfprintf",
            vec![
                Value::Resource(ResourceTable::new().register_stream(
                    StreamFlags::new(true, true, true),
                    StreamMetadata::new("php", "stream", "w+", "php://memory"),
                )),
                Value::string("Foo %$c-0202Sd"),
                Value::packed_array(vec![Value::Int(2)]),
            ],
            &mut output,
        ),
        "Argument number specifier must be greater than zero and less than 2147483647"
    );

    let mut output = OutputBuffer::new();
    let mut resources = ResourceTable::new();
    let stream = resources.register_stream(
        StreamFlags::new(true, true, true),
        StreamMetadata::new("php", "stream", "w+", "php://memory"),
    );
    assert_eq!(
        call(
            "fprintf",
            vec![
                Value::Resource(stream.clone()),
                Value::string("%s:%d"),
                Value::string("id"),
                Value::Int(7)
            ],
            &mut output
        ),
        Value::Int(4)
    );
    assert_eq!(
        call(
            "vfprintf",
            vec![
                Value::Resource(stream.clone()),
                Value::string("|%s:%d|"),
                Value::packed_array(vec![Value::string("next"), Value::Int(8)])
            ],
            &mut output
        ),
        Value::Int(8)
    );
    stream.rewind().expect("memory stream rewind");
    assert_eq!(
        stream.read_to_end().expect("memory stream read"),
        b"id:7|next:8|"
    );

    let mut stdout_output = OutputBuffer::new();
    let stdout = ResourceTable::new().register_stream(
        StreamFlags::new(true, true, true),
        StreamMetadata::new("php", "stream", "w+", "php://stdout"),
    );
    assert_eq!(
        call(
            "fprintf",
            vec![
                Value::Resource(stdout.clone()),
                Value::string("stdout:%d"),
                Value::Int(3)
            ],
            &mut stdout_output
        ),
        Value::Int(8)
    );
    assert_eq!(
        call(
            "fwrite",
            vec![Value::Resource(stdout), Value::string("|tail")],
            &mut stdout_output
        ),
        Value::Int(5)
    );
    assert_eq!(stdout_output.to_string_lossy(), "stdout:3|tail");
}

#[test]
fn math_numeric_builtins_cover_common_paths() {
    let mut output = OutputBuffer::new();

    assert_eq!(
        call("abs", vec![Value::Int(-7)], &mut output),
        Value::Int(7)
    );
    assert_eq!(
        call("abs", vec![Value::string("-2.5")], &mut output),
        Value::float(2.5)
    );
    assert_eq!(
        call(
            "min",
            vec![Value::packed_array(vec![
                Value::Int(3),
                Value::Int(1),
                Value::Int(2)
            ])],
            &mut output
        ),
        Value::Int(1)
    );
    assert_eq!(
        call(
            "max",
            vec![Value::Int(3), Value::Int(1), Value::Int(2)],
            &mut output
        ),
        Value::Int(3)
    );
    assert_eq!(
        call(
            "round",
            vec![Value::float(12.345), Value::Int(2)],
            &mut output
        ),
        Value::float(12.35)
    );
    assert_eq!(
        call("floor", vec![Value::float(3.9)], &mut output),
        Value::float(3.0)
    );
    assert_eq!(
        call("ceil", vec![Value::float(3.1)], &mut output),
        Value::float(4.0)
    );
    assert_eq!(
        call("deg2rad", vec![Value::Int(23)], &mut output),
        Value::float((23.0 / 180.0) * std::f64::consts::PI)
    );
    assert_eq!(
        call(
            "rad2deg",
            vec![Value::float(9_223_372_034_707_292_160.0)],
            &mut output
        ),
        Value::float((9_223_372_034_707_292_160.0 / std::f64::consts::PI) * 180.0)
    );
    assert_eq!(
        call("sqrt", vec![Value::Int(9)], &mut output),
        Value::float(3.0)
    );
    assert_eq!(
        call("pow", vec![Value::Int(2), Value::Int(3)], &mut output),
        Value::Int(8)
    );
    assert!(matches!(
        call(
            "pow",
            vec![Value::Int(i64::MIN), Value::Int(i64::MAX)],
            &mut output
        ),
        Value::Float(value) if value.to_f64().is_infinite() && value.to_f64().is_sign_negative()
    ));
    assert_eq!(
        call("intdiv", vec![Value::Int(7), Value::Int(2)], &mut output),
        Value::Int(3)
    );
    assert_eq!(
        call("fmod", vec![Value::Int(7), Value::Int(2)], &mut output),
        Value::float(1.0)
    );
    assert_eq!(
        call("fdiv", vec![Value::Int(7), Value::Int(2)], &mut output),
        Value::float(3.5)
    );
    assert!(matches!(
        call("fdiv", vec![Value::Int(1), Value::Int(0)], &mut output),
        Value::Float(value) if value.to_f64().is_infinite()
    ));
    assert!(matches!(
        call("fdiv", vec![Value::Int(0), Value::Int(0)], &mut output),
        Value::Float(value) if value.to_f64().is_nan()
    ));
    assert_eq!(
        call("is_finite", vec![Value::float(1.5)], &mut output),
        Value::Bool(true)
    );
    assert_eq!(
        call(
            "is_infinite",
            vec![Value::float(f64::INFINITY)],
            &mut output
        ),
        Value::Bool(true)
    );
    assert_eq!(
        call("is_nan", vec![Value::float(f64::NAN)], &mut output),
        Value::Bool(true)
    );
    assert_eq!(
        call(
            "number_format",
            vec![Value::float(1234.567), Value::Int(2)],
            &mut output
        ),
        Value::string("1,234.57")
    );
    assert_eq!(
        call(
            "number_format",
            vec![
                Value::float(1234.5),
                Value::Int(1),
                Value::string(","),
                Value::string(".")
            ],
            &mut output
        ),
        Value::string("1.234,5")
    );
    assert_eq!(
        call(
            "number_format",
            vec![Value::Int(i64::MAX), Value::Int(5)],
            &mut output
        ),
        Value::string("9,223,372,036,854,775,807.00000")
    );
    assert_eq!(
        call(
            "number_format",
            vec![Value::Int(i64::MAX), Value::Int(0)],
            &mut output
        ),
        Value::string("9,223,372,036,854,775,807")
    );
    assert_eq!(
        call(
            "number_format",
            vec![Value::Int(i64::MAX), Value::Int(-5)],
            &mut output
        ),
        Value::string("9,223,372,036,854,800,000")
    );
    assert_eq!(
        call(
            "number_format",
            vec![Value::float(9_223_372_036_854_775_808.0), Value::Int(-1)],
            &mut output
        ),
        Value::string("9,223,372,036,854,775,808")
    );
}

#[test]
fn math_numeric_builtins_report_value_errors() {
    let entry = BuiltinRegistry::new()
        .get("intdiv")
        .expect("builtin exists");
    let mut output = OutputBuffer::new();
    let mut context = BuiltinContext::new(&mut output);
    let error = (entry.function())(
        &mut context,
        vec![Value::Int(1), Value::Int(0)],
        RuntimeSourceSpan::default(),
    )
    .expect_err("expected value error");
    assert_eq!(error.diagnostic_id(), "E_PHP_RUNTIME_BUILTIN_VALUE");

    let entry = BuiltinRegistry::new().get("fmod").expect("builtin exists");
    let mut output = OutputBuffer::new();
    let mut context = BuiltinContext::new(&mut output);
    assert!(matches!(
        (entry.function())(
            &mut context,
            vec![Value::Int(1), Value::Int(0)],
            RuntimeSourceSpan::default()
        ),
        Ok(Value::Float(value)) if value.to_f64().is_nan()
    ));
}

#[test]
fn array_basic_builtins_cover_keys_values_and_list_checks() {
    let mut output = OutputBuffer::new();
    let mut mixed = PhpArray::new();
    mixed.insert(ArrayKey::Int(1), Value::string("one"));
    mixed.insert(
        ArrayKey::String(PhpString::from_test_str("01")),
        Value::string("zero-one"),
    );
    mixed.insert(
        ArrayKey::String(PhpString::from_test_str("name")),
        Value::string("n"),
    );
    let before = mixed.clone();

    assert_eq!(
        call("count", vec![Value::Array(mixed.clone())], &mut output),
        Value::Int(3)
    );
    assert_eq!(
        call("sizeof", vec![Value::packed_array(vec![])], &mut output),
        Value::Int(0)
    );
    assert_eq!(
        call(
            "array_key_exists",
            vec![Value::string("1"), Value::Array(mixed.clone())],
            &mut output
        ),
        Value::Bool(true)
    );
    assert_eq!(
        call(
            "key_exists",
            vec![Value::string("name"), Value::Array(mixed.clone())],
            &mut output
        ),
        Value::Bool(true)
    );
    assert_eq!(
        call("array_keys", vec![Value::Array(mixed.clone())], &mut output),
        Value::packed_array(vec![
            Value::Int(1),
            Value::string("01"),
            Value::string("name")
        ])
    );
    assert_eq!(
        call(
            "array_values",
            vec![Value::Array(mixed.clone())],
            &mut output
        ),
        Value::packed_array(vec![
            Value::string("one"),
            Value::string("zero-one"),
            Value::string("n")
        ])
    );
    assert_eq!(
        call(
            "array_sum",
            vec![Value::packed_array(vec![
                Value::Int(2),
                Value::string("3"),
                Value::Bool(true)
            ])],
            &mut output
        ),
        Value::Int(6)
    );
    assert_eq!(
        call(
            "array_sum",
            vec![Value::packed_array(vec![Value::Int(2), Value::float(0.5)])],
            &mut output
        ),
        Value::float(2.5)
    );
    assert_eq!(
        call(
            "array_is_list",
            vec![Value::packed_array(vec![Value::Int(1)])],
            &mut output
        ),
        Value::Bool(true)
    );
    assert_eq!(
        call(
            "array_is_list",
            vec![Value::Array(mixed.clone())],
            &mut output
        ),
        Value::Bool(false)
    );
    assert_eq!(
        call(
            "array_key_first",
            vec![Value::Array(mixed.clone())],
            &mut output
        ),
        Value::Int(1)
    );
    assert_eq!(
        call(
            "array_key_last",
            vec![Value::Array(mixed.clone())],
            &mut output
        ),
        Value::string("name")
    );
    assert_eq!(
        call(
            "array_combine",
            vec![
                Value::packed_array(vec![Value::string("x"), Value::Int(2)]),
                Value::packed_array(vec![Value::string("ex"), Value::string("two")])
            ],
            &mut output
        ),
        {
            let mut combined = PhpArray::new();
            combined.insert(
                ArrayKey::String(PhpString::from_test_str("x")),
                Value::string("ex"),
            );
            combined.insert(ArrayKey::Int(2), Value::string("two"));
            Value::Array(combined)
        }
    );
    assert_eq!(mixed, before);
}

#[test]
fn array_basic_builtins_cover_strict_search_and_columns() {
    let mut output = OutputBuffer::new();
    let haystack = Value::packed_array(vec![Value::Int(0), Value::string("7"), Value::Int(7)]);

    assert_eq!(
        call(
            "in_array",
            vec![Value::Int(7), haystack.clone(), Value::Bool(false)],
            &mut output
        ),
        Value::Bool(true)
    );
    assert_eq!(
        call(
            "in_array",
            vec![Value::string("7"), haystack.clone(), Value::Bool(true)],
            &mut output
        ),
        Value::Bool(true)
    );
    assert_eq!(
        call(
            "array_search",
            vec![Value::string("7"), haystack.clone(), Value::Bool(true)],
            &mut output
        ),
        Value::Int(1)
    );
    assert_eq!(
        call(
            "array_search",
            vec![Value::string("missing"), haystack, Value::Bool(false)],
            &mut output
        ),
        Value::Bool(false)
    );

    let mut first = PhpArray::new();
    first.insert(
        ArrayKey::String(PhpString::from_test_str("id")),
        Value::Int(2),
    );
    first.insert(
        ArrayKey::String(PhpString::from_test_str("name")),
        Value::string("Ada"),
    );
    let mut second = PhpArray::new();
    second.insert(
        ArrayKey::String(PhpString::from_test_str("id")),
        Value::Int(3),
    );
    second.insert(
        ArrayKey::String(PhpString::from_test_str("name")),
        Value::string("Grace"),
    );
    let rows = Value::packed_array(vec![Value::Array(first), Value::Array(second)]);

    let mut expected = PhpArray::new();
    expected.insert(ArrayKey::Int(2), Value::string("Ada"));
    expected.insert(ArrayKey::Int(3), Value::string("Grace"));
    assert_eq!(
        call(
            "array_column",
            vec![rows, Value::string("name"), Value::string("id")],
            &mut output
        ),
        Value::Array(expected)
    );
}

#[test]
fn array_unique_preserves_keys_and_honors_comparison_flags() {
    let mut output = OutputBuffer::new();
    let mut input = PhpArray::new();
    input.insert(ArrayKey::Int(10), Value::string("01"));
    input.insert(
        ArrayKey::String(PhpString::from_test_str("one")),
        Value::Int(1),
    );
    input.insert(ArrayKey::Int(11), Value::string("1"));
    input.insert(
        ArrayKey::String(PhpString::from_test_str("upper")),
        Value::string("A"),
    );
    input.insert(
        ArrayKey::String(PhpString::from_test_str("lower")),
        Value::string("a"),
    );

    let mut expected_string = PhpArray::new();
    expected_string.insert(ArrayKey::Int(10), Value::string("01"));
    expected_string.insert(
        ArrayKey::String(PhpString::from_test_str("one")),
        Value::Int(1),
    );
    expected_string.insert(
        ArrayKey::String(PhpString::from_test_str("upper")),
        Value::string("A"),
    );
    expected_string.insert(
        ArrayKey::String(PhpString::from_test_str("lower")),
        Value::string("a"),
    );
    assert_eq!(
        call(
            "array_unique",
            vec![Value::Array(input.clone())],
            &mut output
        ),
        Value::Array(expected_string)
    );

    let mut numeric_input = PhpArray::new();
    numeric_input.insert(ArrayKey::Int(10), Value::string("01"));
    numeric_input.insert(
        ArrayKey::String(PhpString::from_test_str("one")),
        Value::Int(1),
    );
    numeric_input.insert(ArrayKey::Int(11), Value::string("1"));
    let mut expected_numeric = PhpArray::new();
    expected_numeric.insert(ArrayKey::Int(10), Value::string("01"));
    assert_eq!(
        call(
            "array_unique",
            vec![
                Value::Array(numeric_input.clone()),
                Value::Int(SORT_NUMERIC)
            ],
            &mut output
        ),
        Value::Array(expected_numeric.clone())
    );
    assert_eq!(
        call(
            "array_unique",
            vec![Value::Array(numeric_input), Value::Int(SORT_REGULAR)],
            &mut output
        ),
        Value::Array(expected_numeric)
    );

    let mut expected_case = PhpArray::new();
    expected_case.insert(ArrayKey::Int(10), Value::string("01"));
    expected_case.insert(
        ArrayKey::String(PhpString::from_test_str("one")),
        Value::Int(1),
    );
    expected_case.insert(
        ArrayKey::String(PhpString::from_test_str("upper")),
        Value::string("A"),
    );
    assert_eq!(
        call(
            "array_unique",
            vec![
                Value::Array(input),
                Value::Int(SORT_STRING | SORT_FLAG_CASE)
            ],
            &mut output
        ),
        Value::Array(expected_case)
    );
}

#[test]
fn array_intersect_builtins_cover_value_assoc_and_empty_callback_cases() {
    let mut output = OutputBuffer::new();
    let mut first = PhpArray::new();
    first.insert(ArrayKey::Int(0), Value::Int(0));
    first.insert(ArrayKey::Int(1), Value::Int(1));
    first.insert(
        ArrayKey::String(PhpString::from_test_str("two")),
        Value::string("2"),
    );
    let second = Value::packed_array(vec![Value::string("1"), Value::Int(2)]);

    let mut expected = PhpArray::new();
    expected.insert(ArrayKey::Int(1), Value::Int(1));
    expected.insert(
        ArrayKey::String(PhpString::from_test_str("two")),
        Value::string("2"),
    );
    assert_eq!(
        call(
            "array_intersect",
            vec![Value::Array(first.clone()), second],
            &mut output
        ),
        Value::Array(expected)
    );

    let mut assoc_second = PhpArray::new();
    assoc_second.insert(ArrayKey::Int(1), Value::string("1"));
    assoc_second.insert(
        ArrayKey::String(PhpString::from_test_str("two")),
        Value::Int(2),
    );
    let mut expected_assoc = PhpArray::new();
    expected_assoc.insert(ArrayKey::Int(1), Value::Int(1));
    expected_assoc.insert(
        ArrayKey::String(PhpString::from_test_str("two")),
        Value::string("2"),
    );
    assert_eq!(
        call(
            "array_intersect_assoc",
            vec![Value::Array(first.clone()), Value::Array(assoc_second)],
            &mut output
        ),
        Value::Array(expected_assoc)
    );

    let mut key_second = PhpArray::new();
    key_second.insert(ArrayKey::Int(1), Value::string("different"));
    key_second.insert(
        ArrayKey::String(PhpString::from_test_str("two")),
        Value::Bool(false),
    );
    let mut expected_key = PhpArray::new();
    expected_key.insert(ArrayKey::Int(1), Value::Int(1));
    expected_key.insert(
        ArrayKey::String(PhpString::from_test_str("two")),
        Value::string("2"),
    );
    assert_eq!(
        call(
            "array_intersect_key",
            vec![Value::Array(first.clone()), Value::Array(key_second)],
            &mut output
        ),
        Value::Array(expected_key)
    );

    let diff_second = Value::packed_array(vec![Value::string("1")]);
    let mut expected_diff = PhpArray::new();
    expected_diff.insert(ArrayKey::Int(0), Value::Int(0));
    expected_diff.insert(
        ArrayKey::String(PhpString::from_test_str("two")),
        Value::string("2"),
    );
    assert_eq!(
        call(
            "array_diff",
            vec![Value::Array(first.clone()), diff_second],
            &mut output
        ),
        Value::Array(expected_diff)
    );

    let mut assoc_diff_second = PhpArray::new();
    assoc_diff_second.insert(ArrayKey::Int(1), Value::string("1"));
    let mut expected_diff_assoc = PhpArray::new();
    expected_diff_assoc.insert(ArrayKey::Int(0), Value::Int(0));
    expected_diff_assoc.insert(
        ArrayKey::String(PhpString::from_test_str("two")),
        Value::string("2"),
    );
    assert_eq!(
        call(
            "array_diff_assoc",
            vec![Value::Array(first.clone()), Value::Array(assoc_diff_second)],
            &mut output
        ),
        Value::Array(expected_diff_assoc)
    );

    let mut key_diff_second = PhpArray::new();
    key_diff_second.insert(ArrayKey::Int(0), Value::string("different"));
    let mut expected_diff_key = PhpArray::new();
    expected_diff_key.insert(ArrayKey::Int(1), Value::Int(1));
    expected_diff_key.insert(
        ArrayKey::String(PhpString::from_test_str("two")),
        Value::string("2"),
    );
    assert_eq!(
        call(
            "array_diff_key",
            vec![Value::Array(first.clone()), Value::Array(key_diff_second)],
            &mut output
        ),
        Value::Array(expected_diff_key)
    );

    let empty = Value::packed_array(Vec::new());
    for name in [
        "array_intersect_ukey",
        "array_uintersect",
        "array_intersect_uassoc",
    ] {
        assert_eq!(
            call(
                name,
                vec![Value::Array(first.clone()), empty.clone(), Value::Null],
                &mut output
            ),
            Value::packed_array(Vec::new())
        );
    }
    assert_eq!(
        call(
            "array_uintersect_uassoc",
            vec![Value::Array(first), empty, Value::Null, Value::Null],
            &mut output
        ),
        Value::packed_array(Vec::new())
    );
}

#[test]
fn shuffle_mutates_array_by_reference_and_reindexes_values() {
    let mut output = OutputBuffer::new();
    let cell = ReferenceCell::new(Value::Array({
        let mut array = PhpArray::new();
        array.insert(ArrayKey::Int(5), Value::string("a"));
        array.insert(
            ArrayKey::String(PhpString::from_test_str("name")),
            Value::string("b"),
        );
        array.insert(ArrayKey::Int(9), Value::string("c"));
        array
    }));
    assert_eq!(
        call("shuffle", vec![Value::Reference(cell.clone())], &mut output),
        Value::Bool(true)
    );
    let Value::Array(array) = cell.get() else {
        panic!("shuffle should leave an array in the reference cell");
    };
    assert!(array.is_packed_fast());
    assert_eq!(array.len(), 3);
    let mut values = array
        .iter()
        .map(|(_, value)| match value {
            Value::String(value) => value.to_string_lossy(),
            other => panic!("unexpected shuffled value: {other:?}"),
        })
        .collect::<Vec<_>>();
    values.sort();
    assert_eq!(values, ["a", "b", "c"]);
}

#[test]
fn array_pointer_builtins_track_current_key_and_mutating_moves() {
    let mut output = OutputBuffer::new();
    let cell = ReferenceCell::new(Value::Array({
        let mut array = PhpArray::new();
        array.append(Value::string("zero"));
        array.append(Value::string("one"));
        array.insert(ArrayKey::Int(200), Value::string("two"));
        array
    }));

    assert_eq!(
        call("current", vec![cell.get()], &mut output),
        Value::string("zero")
    );
    assert_eq!(call("key", vec![cell.get()], &mut output), Value::Int(0));
    assert_eq!(
        call("next", vec![Value::Reference(cell.clone())], &mut output),
        Value::string("one")
    );
    assert_eq!(call("key", vec![cell.get()], &mut output), Value::Int(1));
    assert_eq!(
        call("end", vec![Value::Reference(cell.clone())], &mut output),
        Value::string("two")
    );
    assert_eq!(call("key", vec![cell.get()], &mut output), Value::Int(200));
    assert_eq!(
        call("prev", vec![Value::Reference(cell.clone())], &mut output),
        Value::string("one")
    );
    assert_eq!(
        call("reset", vec![Value::Reference(cell.clone())], &mut output),
        Value::string("zero")
    );

    let empty = ReferenceCell::new(Value::packed_array(Vec::new()));
    assert_eq!(
        call("current", vec![empty.get()], &mut output),
        Value::Bool(false)
    );
    assert_eq!(call("key", vec![empty.get()], &mut output), Value::Null);
}

#[test]
fn array_range_builtin_covers_numeric_and_string_sequences() {
    let mut output = OutputBuffer::new();

    assert_eq!(
        call("range", vec![Value::Int(1), Value::Int(5)], &mut output),
        Value::packed_array(vec![
            Value::Int(1),
            Value::Int(2),
            Value::Int(3),
            Value::Int(4),
            Value::Int(5)
        ])
    );
    assert_eq!(
        call(
            "range",
            vec![Value::Int(5), Value::Int(1), Value::Int(2)],
            &mut output
        ),
        Value::packed_array(vec![Value::Int(5), Value::Int(3), Value::Int(1)])
    );
    assert_eq!(
        call(
            "range",
            vec![Value::Int(1), Value::Int(2), Value::float(0.5)],
            &mut output
        ),
        Value::packed_array(vec![
            Value::float(1.0),
            Value::float(1.5),
            Value::float(2.0)
        ])
    );
    assert_eq!(
        call(
            "range",
            vec![Value::float(4.5), Value::float(4.2), Value::float(0.1)],
            &mut output
        ),
        Value::packed_array(vec![
            Value::float(4.5),
            Value::float(4.4),
            Value::float(4.3),
            Value::float(4.2)
        ])
    );
    assert_eq!(
        call(
            "range",
            vec![Value::float(9.9), Value::string("0")],
            &mut output
        ),
        Value::packed_array(vec![
            Value::float(9.9),
            Value::float(8.9),
            Value::float(7.9),
            Value::float(6.9),
            Value::float(5.9),
            Value::float(4.9),
            Value::float(3.9000000000000004),
            Value::float(2.9000000000000004),
            Value::float(1.9000000000000004),
            Value::float(0.9000000000000004),
        ])
    );
    assert_eq!(
        call(
            "range",
            vec![Value::string("a"), Value::string("e"), Value::Int(2)],
            &mut output
        ),
        Value::packed_array(vec![
            Value::string("a"),
            Value::string("c"),
            Value::string("e")
        ])
    );
    assert_eq!(
        call(
            "range",
            vec![Value::string("1"), Value::string("3")],
            &mut output
        ),
        Value::packed_array(vec![
            Value::string("1"),
            Value::string("2"),
            Value::string("3")
        ])
    );
    assert_eq!(
        call(
            "range",
            vec![Value::string("1"), Value::string("10"), Value::string("3")],
            &mut output
        ),
        Value::packed_array(vec![
            Value::Int(1),
            Value::Int(4),
            Value::Int(7),
            Value::Int(10)
        ])
    );
}

#[test]
fn array_range_builtin_reports_step_value_errors() {
    let mut output = OutputBuffer::new();

    assert_eq!(
        call_error(
            "range",
            vec![Value::Int(1), Value::Int(7), Value::Int(0)],
            &mut output
        ),
        "range(): Argument #3 ($step) cannot be 0"
    );
    assert_eq!(
        call_error(
            "range",
            vec![
                Value::float(1.0),
                Value::float(7.0),
                Value::float(f64::INFINITY)
            ],
            &mut output
        ),
        "range(): Argument #3 ($step) must be a finite number, INF provided"
    );
    assert_eq!(
        call_error(
            "range",
            vec![Value::Int(1), Value::Int(7), Value::float(7.5)],
            &mut output
        ),
        "range(): Argument #3 ($step) must be less than the range spanned by argument #1 ($start) and argument #2 ($end)"
    );
    assert_eq!(
        call_error(
            "range",
            vec![Value::Int(1), Value::Int(3), Value::Int(-1)],
            &mut output
        ),
        "range(): Argument #3 ($step) must be greater than 0 for increasing ranges"
    );
    assert_eq!(
        call_error(
            "range",
            vec![Value::string("a"), Value::string("c"), Value::Int(-1)],
            &mut output
        ),
        "range(): Argument #3 ($step) must be greater than 0 for increasing ranges"
    );
}

#[test]
fn array_range_builtin_warns_for_invalid_string_inputs() {
    let mut output = OutputBuffer::new();
    assert_eq!(
        call(
            "range",
            vec![Value::string("AA"), Value::string("BB")],
            &mut output
        ),
        Value::packed_array(vec![Value::string("A"), Value::string("B")])
    );
    let warnings = output.to_string_lossy();
    assert!(warnings.contains(
        "range(): Argument #1 ($start) must be a single byte, subsequent bytes are ignored"
    ));
    assert!(warnings.contains(
        "range(): Argument #2 ($end) must be a single byte, subsequent bytes are ignored"
    ));

    let mut output = OutputBuffer::new();
    assert_eq!(
        call(
            "range",
            vec![Value::string("Z"), Value::string("")],
            &mut output
        ),
        Value::packed_array(vec![Value::Int(0)])
    );
    let warnings = output.to_string_lossy();
    assert!(warnings.contains("range(): Argument #2 ($end) must not be empty, casted to 0"));
    assert!(warnings.contains(
            "range(): Argument #2 ($end) must be a single byte string if argument #1 ($start) is a single byte string, argument #1 ($start) converted to 0"
        ));

    let mut output = OutputBuffer::new();
    assert_eq!(
        call(
            "range",
            vec![Value::string("A"), Value::string("H"), Value::float(2.6)],
            &mut output
        ),
        Value::packed_array(vec![Value::float(0.0)])
    );
    assert!(output.to_string_lossy().contains(
            "range(): Argument #3 ($step) must be of type int when generating an array of characters, inputs converted to 0"
        ));

    let mut output = OutputBuffer::new();
    assert_eq!(
        call(
            "range",
            vec![Value::string("1"), Value::string("2"), Value::float(0.1)],
            &mut output
        ),
        Value::packed_array(vec![
            Value::float(1.0),
            Value::float(1.1),
            Value::float(1.2),
            Value::float(1.3),
            Value::float(1.4),
            Value::float(1.5),
            Value::float(1.6),
            Value::float(1.7000000000000002),
            Value::float(1.8),
            Value::float(1.9),
            Value::float(2.0)
        ])
    );
    assert!(output.is_empty());
}

#[test]
fn array_range_builtin_deprecates_null_boundaries() {
    let mut output = OutputBuffer::new();
    assert_eq!(
        call("range", vec![Value::Null, Value::Null], &mut output),
        Value::packed_array(vec![Value::Int(0)])
    );
    let warnings = output.to_string_lossy();
    assert!(warnings.contains(
        "range(): Passing null to parameter #1 ($start) of type string|int|float is deprecated"
    ));
    assert!(warnings.contains(
        "range(): Passing null to parameter #2 ($end) of type string|int|float is deprecated"
    ));

    let mut output = OutputBuffer::new();
    assert_eq!(
        call("range", vec![Value::Null, Value::string("e")], &mut output),
        Value::packed_array(vec![Value::Int(0)])
    );
    let warnings = output.to_string_lossy();
    assert!(warnings.contains(
        "range(): Passing null to parameter #1 ($start) of type string|int|float is deprecated"
    ));
    assert!(warnings.contains(
            "range(): Argument #1 ($start) must be a single byte string if argument #2 ($end) is a single byte string, argument #2 ($end) converted to 0"
        ));
}

#[test]
fn array_range_builtin_reports_oversized_ranges_without_panicking() {
    let mut output = OutputBuffer::new();

    assert_eq!(
        call_error(
            "range",
            vec![Value::float(1.0), Value::float(f64::INFINITY)],
            &mut output
        ),
        "range(): Argument #2 ($end) must be a finite number, INF provided"
    );
    let error = call_error(
        "range",
        vec![Value::Int(i64::MIN), Value::Int(i64::MAX), Value::Int(1)],
        &mut output,
    );
    assert!(error.contains("The supplied range exceeds the maximum array size by "));
    assert!(error.contains("start=-9223372036854775808, end=9223372036854775807, step=1"));
    assert!(error.contains("Maximum size: 1000000."));
    assert_eq!(
        call_error(
            "range",
            vec![Value::Int(1), Value::Int(3), Value::Int(i64::MIN)],
            &mut output
        ),
        "range(): Argument #3 ($step) must be greater than 0 for increasing ranges"
    );
}

#[test]
fn array_stack_builtins_mutate_only_references() {
    let mut output = OutputBuffer::new();
    let cell = ReferenceCell::new(Value::packed_array(vec![Value::Int(1), Value::Int(2)]));

    assert_eq!(
        call(
            "array_push",
            vec![Value::Reference(cell.clone()), Value::Int(3)],
            &mut output
        ),
        Value::Int(3)
    );
    assert_eq!(
        cell.get(),
        Value::packed_array(vec![Value::Int(1), Value::Int(2), Value::Int(3)])
    );
    assert_eq!(
        call(
            "array_pop",
            vec![Value::Reference(cell.clone())],
            &mut output
        ),
        Value::Int(3)
    );
    assert_eq!(
        call(
            "array_unshift",
            vec![Value::Reference(cell.clone()), Value::Int(0)],
            &mut output
        ),
        Value::Int(3)
    );
    assert_eq!(
        call(
            "array_shift",
            vec![Value::Reference(cell.clone())],
            &mut output
        ),
        Value::Int(0)
    );
    assert_eq!(
        cell.get(),
        Value::packed_array(vec![Value::Int(1), Value::Int(2)])
    );
}

#[test]
fn array_slice_merge_and_transform_builtins_work() {
    let mut output = OutputBuffer::new();
    let mut keyed = PhpArray::new();
    keyed.insert(ArrayKey::Int(2), Value::string("two"));
    keyed.insert(
        ArrayKey::String(PhpString::from_test_str("a")),
        Value::Int(1),
    );
    keyed.insert(ArrayKey::Int(4), Value::string("four"));

    let mut expected_slice = PhpArray::new();
    expected_slice.insert(
        ArrayKey::String(PhpString::from_test_str("a")),
        Value::Int(1),
    );
    expected_slice.append(Value::string("four"));
    assert_eq!(
        call(
            "array_slice",
            vec![Value::Array(keyed.clone()), Value::Int(1), Value::Int(2)],
            &mut output
        ),
        Value::Array(expected_slice)
    );
    let mut expected_reverse = PhpArray::new();
    expected_reverse.append(Value::string("four"));
    expected_reverse.insert(
        ArrayKey::String(PhpString::from_test_str("a")),
        Value::Int(1),
    );
    expected_reverse.append(Value::string("two"));
    assert_eq!(
        call(
            "array_reverse",
            vec![Value::Array(keyed.clone()), Value::Bool(false)],
            &mut output
        ),
        Value::Array(expected_reverse)
    );
    assert_eq!(
        call(
            "array_pad",
            vec![
                Value::packed_array(vec![Value::Int(1)]),
                Value::Int(3),
                Value::Int(0)
            ],
            &mut output
        ),
        Value::packed_array(vec![Value::Int(1), Value::Int(0), Value::Int(0)])
    );
    let mut expected_fill = PhpArray::new();
    expected_fill.insert(ArrayKey::Int(-2), Value::string("x"));
    expected_fill.insert(ArrayKey::Int(-1), Value::string("x"));
    expected_fill.insert(ArrayKey::Int(0), Value::string("x"));
    assert_eq!(
        call(
            "array_fill",
            vec![Value::Int(-2), Value::Int(3), Value::string("x")],
            &mut output
        ),
        Value::Array(expected_fill)
    );
    let mut expected_fill_keys = PhpArray::new();
    expected_fill_keys.insert(
        ArrayKey::String(PhpString::from_test_str("name")),
        Value::Bool(true),
    );
    expected_fill_keys.insert(ArrayKey::Int(2), Value::Bool(true));
    expected_fill_keys.insert(
        ArrayKey::String(PhpString::from_test_str("1.5")),
        Value::Bool(true),
    );
    expected_fill_keys.insert(
        ArrayKey::String(PhpString::from_test_str("")),
        Value::Bool(true),
    );
    assert_eq!(
        call(
            "array_fill_keys",
            vec![
                Value::packed_array(vec![
                    Value::string("name"),
                    Value::string("2"),
                    Value::float(1.5),
                    Value::Bool(false),
                    Value::Null,
                ]),
                Value::Bool(true),
            ],
            &mut output
        ),
        Value::Array(expected_fill_keys)
    );
    assert_eq!(
        call_error(
            "array_fill",
            vec![Value::Int(0), Value::Int(-1), Value::Null],
            &mut output
        ),
        "array_fill(): Argument #2 ($count) must be greater than or equal to 0"
    );

    let mut left = PhpArray::new();
    left.insert(ArrayKey::Int(0), Value::string("x"));
    left.insert(
        ArrayKey::String(PhpString::from_test_str("k")),
        Value::Int(1),
    );
    let mut right = PhpArray::new();
    right.insert(ArrayKey::Int(7), Value::string("y"));
    right.insert(
        ArrayKey::String(PhpString::from_test_str("k")),
        Value::Int(2),
    );
    let mut expected_merge = PhpArray::new();
    expected_merge.append(Value::string("x"));
    expected_merge.insert(
        ArrayKey::String(PhpString::from_test_str("k")),
        Value::Int(2),
    );
    expected_merge.append(Value::string("y"));
    assert_eq!(
        call(
            "array_merge",
            vec![Value::Array(left.clone()), Value::Array(right.clone())],
            &mut output
        ),
        Value::Array(expected_merge)
    );

    let mut expected_replace = keyed.clone();
    expected_replace.insert(ArrayKey::Int(7), Value::string("y"));
    expected_replace.insert(
        ArrayKey::String(PhpString::from_test_str("k")),
        Value::Int(2),
    );
    assert_eq!(
        call(
            "array_replace",
            vec![Value::Array(keyed), Value::Array(right)],
            &mut output
        ),
        Value::Array(expected_replace)
    );

    let mut rand_input = PhpArray::new();
    rand_input.insert(ArrayKey::Int(2), Value::string("two"));
    rand_input.insert(
        ArrayKey::String(PhpString::from_test_str("name")),
        Value::string("n"),
    );
    let rand_key = call(
        "array_rand",
        vec![Value::Array(rand_input.clone())],
        &mut output,
    );
    assert!(
        matches!(rand_key, Value::Int(2))
            || matches!(rand_key, Value::String(ref key) if key.as_bytes() == b"name")
    );
    let rand_keys = call(
        "array_rand",
        vec![Value::Array(rand_input), Value::Int(2)],
        &mut output,
    );
    let Value::Array(rand_keys) = rand_keys else {
        panic!("array_rand with num > 1 should return a packed array");
    };
    assert_eq!(rand_keys.len(), 2);
    let mut returned = rand_keys
        .iter()
        .map(|(_, value)| match value {
            Value::Int(value) => format!("int:{value}"),
            Value::String(value) => format!("str:{}", value.to_string_lossy()),
            other => panic!("unexpected array_rand key value: {other:?}"),
        })
        .collect::<Vec<_>>();
    returned.sort();
    assert_eq!(returned, ["int:2", "str:name"]);
    assert_eq!(
        call_error("array_rand", vec![Value::packed_array(vec![])], &mut output),
        "builtin array_rand: Array is empty"
    );

    let mut nested_left = PhpArray::new();
    nested_left.insert(ArrayKey::Int(0), Value::string("keep"));
    nested_left.insert(
        ArrayKey::String(PhpString::from_test_str("inner")),
        Value::Int(1),
    );
    let mut recursive_left = PhpArray::new();
    recursive_left.insert(
        ArrayKey::String(PhpString::from_test_str("nested")),
        Value::Array(nested_left),
    );
    recursive_left.insert(ArrayKey::Int(2), Value::string("old"));
    let mut nested_right = PhpArray::new();
    nested_right.insert(
        ArrayKey::String(PhpString::from_test_str("inner")),
        Value::Int(2),
    );
    nested_right.insert(
        ArrayKey::String(PhpString::from_test_str("added")),
        Value::Bool(true),
    );
    let mut recursive_right = PhpArray::new();
    recursive_right.insert(
        ArrayKey::String(PhpString::from_test_str("nested")),
        Value::Array(nested_right),
    );
    recursive_right.insert(ArrayKey::Int(2), Value::string("new"));
    let mut expected_nested = PhpArray::new();
    expected_nested.insert(ArrayKey::Int(0), Value::string("keep"));
    expected_nested.insert(
        ArrayKey::String(PhpString::from_test_str("inner")),
        Value::Int(2),
    );
    expected_nested.insert(
        ArrayKey::String(PhpString::from_test_str("added")),
        Value::Bool(true),
    );
    let mut expected_recursive = PhpArray::new();
    expected_recursive.insert(
        ArrayKey::String(PhpString::from_test_str("nested")),
        Value::Array(expected_nested),
    );
    expected_recursive.insert(ArrayKey::Int(2), Value::string("new"));
    assert_eq!(
        call(
            "array_replace_recursive",
            vec![Value::Array(recursive_left), Value::Array(recursive_right)],
            &mut output
        ),
        Value::Array(expected_recursive)
    );
}

#[test]
fn array_splice_chunk_flip_and_recursive_merge_work() {
    let mut output = OutputBuffer::new();
    let cell = ReferenceCell::new(Value::packed_array(vec![
        Value::string("a"),
        Value::string("b"),
        Value::string("c"),
    ]));
    assert_eq!(
        call(
            "array_splice",
            vec![
                Value::Reference(cell.clone()),
                Value::Int(1),
                Value::Int(1),
                Value::packed_array(vec![Value::string("x")])
            ],
            &mut output
        ),
        Value::packed_array(vec![Value::string("b")])
    );
    assert_eq!(
        cell.get(),
        Value::packed_array(vec![
            Value::string("a"),
            Value::string("x"),
            Value::string("c")
        ])
    );

    assert_eq!(
        call(
            "array_chunk",
            vec![
                Value::packed_array(vec![Value::Int(1), Value::Int(2), Value::Int(3)]),
                Value::Int(2)
            ],
            &mut output
        ),
        Value::packed_array(vec![
            Value::packed_array(vec![Value::Int(1), Value::Int(2)]),
            Value::packed_array(vec![Value::Int(3)])
        ])
    );
    let mut keyed_chunk_input = PhpArray::new();
    keyed_chunk_input.insert(
        ArrayKey::String(PhpString::from_test_str("key1")),
        Value::Int(1),
    );
    keyed_chunk_input.insert(
        ArrayKey::String(PhpString::from_test_str("key2")),
        Value::Int(2),
    );
    keyed_chunk_input.insert(
        ArrayKey::String(PhpString::from_test_str("key3")),
        Value::Int(3),
    );
    assert_eq!(
        call(
            "array_chunk",
            vec![Value::Array(keyed_chunk_input.clone()), Value::Int(2)],
            &mut output
        ),
        Value::packed_array(vec![
            Value::packed_array(vec![Value::Int(1), Value::Int(2)]),
            Value::packed_array(vec![Value::Int(3)])
        ])
    );
    let mut expected_preserved_chunk = PhpArray::new();
    expected_preserved_chunk.insert(
        ArrayKey::String(PhpString::from_test_str("key1")),
        Value::Int(1),
    );
    expected_preserved_chunk.insert(
        ArrayKey::String(PhpString::from_test_str("key2")),
        Value::Int(2),
    );
    let mut expected_preserved_tail = PhpArray::new();
    expected_preserved_tail.insert(
        ArrayKey::String(PhpString::from_test_str("key3")),
        Value::Int(3),
    );
    assert_eq!(
        call(
            "array_chunk",
            vec![
                Value::Array(keyed_chunk_input),
                Value::Int(2),
                Value::Bool(true)
            ],
            &mut output
        ),
        Value::packed_array(vec![
            Value::Array(expected_preserved_chunk),
            Value::Array(expected_preserved_tail)
        ])
    );

    let mut flip_input = PhpArray::new();
    flip_input.insert(
        ArrayKey::String(PhpString::from_test_str("a")),
        Value::Int(1),
    );
    flip_input.insert(
        ArrayKey::String(PhpString::from_test_str("b")),
        Value::string("x"),
    );
    let mut expected_flip = PhpArray::new();
    expected_flip.insert(ArrayKey::Int(1), Value::string("a"));
    expected_flip.insert(
        ArrayKey::String(PhpString::from_test_str("x")),
        Value::string("b"),
    );
    assert_eq!(
        call("array_flip", vec![Value::Array(flip_input)], &mut output),
        Value::Array(expected_flip)
    );
    let mut flip_reference_input = PhpArray::new();
    flip_reference_input.insert(
        ArrayKey::String(PhpString::from_test_str("template")),
        Value::Reference(ReferenceCell::new(Value::string("Page No Title"))),
    );
    let mut expected_reference_flip = PhpArray::new();
    expected_reference_flip.insert(
        ArrayKey::String(PhpString::from_test_str("Page No Title")),
        Value::string("template"),
    );
    assert_eq!(
        call(
            "array_flip",
            vec![Value::Array(flip_reference_input)],
            &mut output
        ),
        Value::Array(expected_reference_flip)
    );
    let mut flip_skip_input = PhpArray::new();
    flip_skip_input.insert(
        ArrayKey::String(PhpString::from_test_str("d")),
        Value::Bool(true),
    );
    flip_skip_input.insert(
        ArrayKey::String(PhpString::from_test_str("E")),
        Value::Bool(false),
    );
    flip_skip_input.insert(ArrayKey::String(PhpString::from_test_str("F")), Value::Null);
    flip_skip_input.insert(ArrayKey::Int(0), Value::string("G"));
    let diagnostics = {
        let mut context = BuiltinContext::new(&mut output);
        assert_eq!(
            call_in_context(
                &mut context,
                "array_flip",
                vec![Value::Array(flip_skip_input)]
            ),
            Value::Array({
                let mut expected = PhpArray::new();
                expected.insert(
                    ArrayKey::String(PhpString::from_test_str("G")),
                    Value::Int(0),
                );
                expected
            })
        );
        context.take_diagnostics()
    };
    assert_eq!(diagnostics.len(), 3);
    assert!(
        diagnostics
            .iter()
            .all(|diagnostic| diagnostic.id() == "E_PHP_RUNTIME_ARRAY_FLIP_ENTRY_SKIPPED")
    );

    let mut first = PhpArray::new();
    first.insert(
        ArrayKey::String(PhpString::from_test_str("k")),
        Value::Int(1),
    );
    let mut second = PhpArray::new();
    second.insert(
        ArrayKey::String(PhpString::from_test_str("k")),
        Value::Int(2),
    );
    let mut expected = PhpArray::new();
    expected.insert(
        ArrayKey::String(PhpString::from_test_str("k")),
        Value::packed_array(vec![Value::Int(1), Value::Int(2)]),
    );
    assert_eq!(
        call(
            "array_merge_recursive",
            vec![Value::Array(first), Value::Array(second)],
            &mut output
        ),
        Value::Array(expected)
    );
}

#[test]
fn serialization_builtins_roundtrip_and_fail_closed() {
    let mut output = OutputBuffer::new();

    assert_eq!(
        call("serialize", vec![Value::Int(1)], &mut output),
        Value::string("i:1;")
    );
    assert_eq!(
        call("unserialize", vec![Value::string("i:1;")], &mut output),
        Value::Int(1)
    );
    assert_eq!(
        call(
            "unserialize",
            vec![Value::string("bad payload")],
            &mut output
        ),
        Value::Bool(false)
    );
}

#[test]
fn setlocale_reports_supported_c_locale_and_rejects_missing_locales() {
    let mut output = OutputBuffer::new();

    assert_eq!(
        call(
            "setlocale",
            vec![Value::Int(6), Value::string("C")],
            &mut output
        ),
        Value::string("C")
    );
    assert_eq!(
        call(
            "setlocale",
            vec![Value::Int(6), Value::string("invalid")],
            &mut output
        ),
        Value::Bool(false)
    );
    assert_eq!(
        call(
            "setlocale",
            vec![Value::Int(0), Value::string("fr_FR")],
            &mut output
        ),
        Value::Bool(false)
    );
}
