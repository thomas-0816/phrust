use std::{
    fs,
    io::{BufRead, BufReader, Read, Write},
    net::TcpStream,
    process::{Child, Command as Proc, Stdio},
    sync::{Arc, Barrier},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

#[test]
fn server_serves_healthz() {
    let docroot = temp_docroot();
    let mut child = start_server(&docroot, &[]);

    let address = read_listening_address(&mut child);
    let response = http_get(&address, "/healthz");

    stop_child(child);
    fs::remove_dir_all(docroot).expect("remove temp docroot");

    assert!(response.starts_with("HTTP/1.1 200 OK"), "{response}");
    assert!(response.ends_with("ok\n"), "{response}");
}

#[test]
fn server_serves_static_file_and_head() {
    let docroot = temp_docroot();
    fs::write(docroot.join("static.txt"), "static bytes\n").expect("write static fixture");
    let mut child = start_server(&docroot, &[]);

    let address = read_listening_address(&mut child);
    let get_response = http_request(&address, "GET", "/static.txt");
    let head_response = http_request(&address, "HEAD", "/static.txt");

    stop_child(child);
    fs::remove_dir_all(docroot).expect("remove temp docroot");

    assert!(
        get_response.starts_with("HTTP/1.1 200 OK"),
        "{get_response}"
    );
    assert!(
        get_response.contains("content-length: 13"),
        "{get_response}"
    );
    assert!(get_response.ends_with("static bytes\n"), "{get_response}");
    assert!(
        head_response.starts_with("HTTP/1.1 200 OK"),
        "{head_response}"
    );
    assert!(
        head_response.contains("content-length: 13"),
        "{head_response}"
    );
    assert!(
        !head_response.ends_with("static bytes\n"),
        "{head_response}"
    );
    assert_response_contains_header(&get_response, "accept-ranges", "bytes");
    assert_eq!(response_header_values(&get_response, "etag").len(), 1);
    assert_eq!(
        response_header_values(&get_response, "last-modified").len(),
        1
    );
}

#[test]
fn server_static_conditional_requests_return_304() {
    let docroot = temp_docroot();
    fs::write(docroot.join("static.txt"), "static bytes\n").expect("write static fixture");
    let mut child = start_server(&docroot, &[]);

    let address = read_listening_address(&mut child);
    let first = http_request(&address, "GET", "/static.txt");
    let etag = response_header_values(&first, "etag")[0].to_string();
    let last_modified = response_header_values(&first, "last-modified")[0].to_string();
    let etag_response = http_request_with_headers(
        &address,
        "GET",
        "/static.txt",
        &[("If-None-Match", &etag)],
        "",
    );
    let modified_response = http_request_with_headers(
        &address,
        "GET",
        "/static.txt",
        &[("If-Modified-Since", &last_modified)],
        "",
    );

    stop_child(child);
    fs::remove_dir_all(docroot).expect("remove temp docroot");

    assert!(
        etag_response.starts_with("HTTP/1.1 304 Not Modified"),
        "{etag_response}"
    );
    assert_eq!(response_body(&etag_response), "");
    assert!(
        modified_response.starts_with("HTTP/1.1 304 Not Modified"),
        "{modified_response}"
    );
    assert_eq!(response_body(&modified_response), "");
}

#[test]
fn server_static_range_requests_return_partial_content() {
    let docroot = temp_docroot();
    fs::write(docroot.join("static.txt"), "abcdef").expect("write static fixture");
    let mut child = start_server(&docroot, &[]);

    let address = read_listening_address(&mut child);
    let partial = http_request_with_headers(
        &address,
        "GET",
        "/static.txt",
        &[("Range", "bytes=1-3")],
        "",
    );
    let suffix =
        http_request_with_headers(&address, "GET", "/static.txt", &[("Range", "bytes=-2")], "");
    let invalid = http_request_with_headers(
        &address,
        "GET",
        "/static.txt",
        &[("Range", "bytes=20-30")],
        "",
    );

    stop_child(child);
    fs::remove_dir_all(docroot).expect("remove temp docroot");

    assert!(
        partial.starts_with("HTTP/1.1 206 Partial Content"),
        "{partial}"
    );
    assert_response_contains_header(&partial, "content-range", "bytes 1-3/6");
    assert_response_contains_header(&partial, "content-length", "3");
    assert_eq!(response_body(&partial), "bcd");
    assert!(
        suffix.starts_with("HTTP/1.1 206 Partial Content"),
        "{suffix}"
    );
    assert_eq!(response_body(&suffix), "ef");
    assert!(
        invalid.starts_with("HTTP/1.1 416 Range Not Satisfiable"),
        "{invalid}"
    );
    assert_response_contains_header(&invalid, "content-range", "bytes */6");
    assert_response_contains_header(&invalid, "content-length", "0");
}

#[test]
fn server_selects_precompressed_static_assets_when_accepted() {
    let docroot = temp_docroot();
    fs::write(docroot.join("app.js"), "plain asset\n").expect("write static fixture");
    fs::write(docroot.join("app.js.gz"), "precompressed asset\n").expect("write gzip fixture");
    let mut child = start_server(&docroot, &[]);

    let address = read_listening_address(&mut child);
    let response = http_request_with_headers(
        &address,
        "GET",
        "/app.js",
        &[("Accept-Encoding", "gzip")],
        "",
    );

    stop_child(child);
    fs::remove_dir_all(docroot).expect("remove temp docroot");

    assert!(response.starts_with("HTTP/1.1 200 OK"), "{response}");
    assert_response_contains_header(&response, "content-encoding", "gzip");
    assert_response_contains_header(&response, "vary", "Accept-Encoding");
    assert_response_contains_header(
        &response,
        "content-type",
        "application/javascript; charset=UTF-8",
    );
    assert_eq!(response_body(&response), "precompressed asset\n");
}

#[test]
fn server_reports_static_file_metrics() {
    let docroot = temp_docroot();
    fs::write(docroot.join("static.txt"), "abcdef").expect("write static fixture");
    fs::write(docroot.join("static.txt.gz"), "gzipped").expect("write gzip fixture");
    let mut child = start_server(&docroot, &[]);

    let address = read_listening_address(&mut child);
    let first = http_request(&address, "GET", "/static.txt");
    let etag = response_header_values(&first, "etag")[0].to_string();
    let _ = http_request_with_headers(
        &address,
        "GET",
        "/static.txt",
        &[("If-None-Match", &etag)],
        "",
    );
    let _ = http_request_with_headers(
        &address,
        "GET",
        "/static.txt",
        &[("Range", "bytes=0-1")],
        "",
    );
    let _ = http_request_with_headers(
        &address,
        "GET",
        "/static.txt",
        &[("Accept-Encoding", "gzip")],
        "",
    );
    let metrics = http_request(&address, "GET", "/__phrust/metrics");

    stop_child(child);
    fs::remove_dir_all(docroot).expect("remove temp docroot");

    assert!(
        metrics.contains("phrust_server_static_streamed_bytes_total 15"),
        "{metrics}"
    );
    assert!(
        metrics.contains("phrust_server_static_not_modified_total 1"),
        "{metrics}"
    );
    assert!(
        metrics.contains("phrust_server_static_partial_responses_total 1"),
        "{metrics}"
    );
    assert!(
        metrics.contains("phrust_server_static_precompressed_hits_total 1"),
        "{metrics}"
    );
}

#[test]
fn server_never_serves_php_scripts_as_static_source() {
    let docroot = temp_docroot();
    fs::write(
        docroot.join("source.php"),
        "<?php echo \"executed\\n\"; // static-source-marker\n",
    )
    .expect("write php fixture");
    let mut child = start_server(&docroot, &[]);

    let address = read_listening_address(&mut child);
    let response = http_request(&address, "GET", "/source.php");

    stop_child(child);
    fs::remove_dir_all(docroot).expect("remove temp docroot");

    assert!(response.starts_with("HTTP/1.1 200 OK"), "{response}");
    assert_eq!(response_body(&response), "executed\n");
    assert!(!response.contains("static-source-marker"), "{response}");
}

#[test]
fn server_exposes_internal_metrics() {
    let docroot = fixture_docroot("fixtures/server/php");
    let mut child = start_server(&docroot, &[]);

    let address = read_listening_address(&mut child);
    let _ = http_request(&address, "GET", "/hello.php");
    let response = http_request(&address, "GET", "/__phrust/metrics");

    stop_child(child);

    assert!(response.starts_with("HTTP/1.1 200 OK"), "{response}");
    assert!(
        response.contains("# phrust-server MVP internal metrics"),
        "{response}"
    );
    assert!(
        response.contains("phrust_server_requests_total"),
        "{response}"
    );
    assert!(
        response.contains("phrust_server_php_responses_total"),
        "{response}"
    );
    assert!(
        response.contains("phrust_server_script_cache_hits_total"),
        "{response}"
    );
    assert!(
        response.contains("phrust_server_script_cache_stale_invalidations_total"),
        "{response}"
    );
    assert!(
        response.contains("phrust_server_script_cache_compile_errors_total"),
        "{response}"
    );
    assert!(
        response.contains("phrust_server_include_resolution_hits_total"),
        "{response}"
    );
    assert!(
        response.contains("phrust_server_include_compile_hits_total"),
        "{response}"
    );
    assert!(
        response.contains("phrust_server_persistent_engine_policy_reuses_total"),
        "{response}"
    );
    assert!(
        response.contains("phrust_server_persistent_engine_immutable_metadata_reuses_total"),
        "{response}"
    );
    assert!(
        response.contains("phrust_server_persistent_engine_request_local_resets_total"),
        "{response}"
    );
    assert!(
        response.contains(
            "phrust_server_persistent_engine_rejected_persistence_total{reason=\"request_local_state\"}"
        ),
        "{response}"
    );
}

#[test]
fn server_reuses_compiled_script_cache_for_repeated_php_requests() {
    let docroot = fixture_docroot("fixtures/server/php");
    let mut child = start_server(&docroot, &[]);

    let address = read_listening_address(&mut child);
    let first_response = http_request(&address, "GET", "/hello.php");
    let second_response = http_request(&address, "GET", "/hello.php");
    let metrics = http_request(&address, "GET", "/__phrust/metrics");

    stop_child(child);

    assert!(
        first_response.starts_with("HTTP/1.1 200 OK"),
        "{first_response}"
    );
    assert!(
        second_response.starts_with("HTTP/1.1 200 OK"),
        "{second_response}"
    );
    assert!(
        metrics.contains("phrust_server_script_cache_hits_total 1\n"),
        "{metrics}"
    );
    assert!(
        metrics.contains("phrust_server_script_cache_misses_total 1\n"),
        "{metrics}"
    );
    assert!(
        metrics.contains("phrust_server_persistent_engine_immutable_metadata_reuses_total 1\n"),
        "{metrics}"
    );
    assert!(
        metrics.contains("phrust_server_persistent_engine_request_local_resets_total 2\n"),
        "{metrics}"
    );
    assert!(
        metrics.contains(
            "phrust_server_persistent_engine_rejected_persistence_total{reason=\"request_local_state\"} 2\n"
        ),
        "{metrics}"
    );
}

#[test]
fn server_protects_metrics_with_configured_token() {
    let docroot = fixture_docroot("fixtures/server/php");
    let mut child = start_server(&docroot, &["--metrics-token", "secret"]);

    let address = read_listening_address(&mut child);
    let forbidden = http_request(&address, "GET", "/__phrust/metrics");
    let authorized = http_request_with_headers(
        &address,
        "GET",
        "/__phrust/metrics",
        &[("Authorization", "Bearer secret")],
        "",
    );

    stop_child(child);

    assert!(
        forbidden.starts_with("HTTP/1.1 403 Forbidden"),
        "{forbidden}"
    );
    assert!(authorized.starts_with("HTTP/1.1 200 OK"), "{authorized}");
    assert!(
        authorized.contains("phrust_server_requests_total"),
        "{authorized}"
    );
}

#[test]
fn server_writes_compact_access_log_line() {
    let docroot = temp_docroot();
    let log_path = docroot.join("access.log");
    let log_arg = log_path.to_string_lossy().to_string();
    fs::write(docroot.join("static.txt"), "static bytes\n").expect("write static fixture");
    let mut child = start_server(&docroot, &["--access-log", &log_arg]);

    let address = read_listening_address(&mut child);
    let response = http_request(&address, "GET", "/static.txt?cache=1");

    stop_child(child);
    let log = fs::read_to_string(&log_path).expect("read access log");
    fs::remove_dir_all(docroot).expect("remove temp docroot");

    assert!(response.starts_with("HTTP/1.1 200 OK"), "{response}");
    assert!(log.contains("method=GET"), "{log}");
    assert!(log.contains("path=\"/static.txt?cache=1\""), "{log}");
    assert!(log.contains("status=200"), "{log}");
    assert!(log.contains("bytes=13"), "{log}");
    assert!(log.contains("route=static"), "{log}");
    assert!(log.contains("cache=-"), "{log}");
}

#[test]
fn server_debug_log_records_request_timeline_and_redacts_secrets() {
    let docroot = fixture_docroot("fixtures/server/php");
    let debug_dir = temp_docroot();
    let debug_log = debug_dir.join("server-debug.jsonl");
    let debug_log_arg = debug_log.to_string_lossy().to_string();
    let mut child = start_server(
        &docroot,
        &[
            "--debug",
            "--error-format",
            "json",
            "--debug-log",
            &debug_log_arg,
        ],
    );

    let address = read_listening_address(&mut child);
    let response = http_request_with_headers(
        &address,
        "GET",
        "/hello.php?token=secret-token",
        &[
            ("Authorization", "Bearer secret-token"),
            ("Cookie", "PHPSESSID=session-secret"),
        ],
        "",
    );

    stop_child(child);
    let log = fs::read_to_string(&debug_log).expect("read debug log");
    fs::remove_dir_all(debug_dir).expect("remove debug temp dir");

    assert!(response.starts_with("HTTP/1.1 200 OK"), "{response}");
    assert!(log.contains("\"request_id\":\"req-00000001\""), "{log}");
    assert!(log.contains("D_PHRUST_SERVER_REQUEST_ACCEPTED"), "{log}");
    assert!(log.contains("D_PHRUST_SERVER_ROUTE_RESOLVED"), "{log}");
    assert!(log.contains("D_PHRUST_SERVER_SCRIPT_CACHE_END"), "{log}");
    assert!(log.contains("D_PHRUST_SERVER_EXECUTE_END"), "{log}");
    assert!(log.contains("D_PHRUST_SERVER_RESPONSE"), "{log}");
    assert!(log.contains("\"cache_hit\":\"false\""), "{log}");
    assert!(log.contains("\"status\":\"200\""), "{log}");
    assert!(!log.contains("Bearer secret-token"), "{log}");
    assert!(!log.contains("session-secret"), "{log}");
    assert!(!log.contains("secret-token"), "{log}");

    for line in log.lines() {
        let value: serde_json::Value = serde_json::from_str(line).expect("debug JSON line");
        for key in [
            "kind",
            "schema_version",
            "code",
            "layer",
            "phase",
            "message",
        ] {
            assert!(value.get(key).is_some(), "missing {key} in {line}");
        }
        assert_eq!(value["kind"], "debug_event");
    }
}

#[test]
fn server_debug_log_contains_request_failure_diagnostics_without_secrets() {
    let docroot = temp_docroot();
    fs::write(docroot.join("index.php"), "<?php echo 'ok';").expect("write index");
    let outside = docroot.with_extension("outside-diagnostic");
    fs::write(&outside, "secret").expect("write outside file");
    std::os::unix::fs::symlink(&outside, docroot.join("link.php")).expect("create symlink");
    let debug_dir = temp_docroot();
    let debug_log = debug_dir.join("server-debug.jsonl");
    let debug_log_arg = debug_log.to_string_lossy().to_string();
    let mut child = start_server(
        &docroot,
        &[
            "--debug",
            "--error-format",
            "json",
            "--debug-log",
            &debug_log_arg,
        ],
    );

    let address = read_listening_address(&mut child);
    let missing = http_request_with_headers(
        &address,
        "GET",
        "/missing.php?token=secret-token",
        &[("Cookie", "PHPSESSID=session-secret")],
        "",
    );
    let forbidden = http_request(&address, "GET", "/link.php");
    let multipart = http_request_with_headers(
        &address,
        "POST",
        "/index.php",
        &[
            ("Content-Type", "multipart/form-data"),
            ("Content-Length", "24"),
            ("Cookie", "PHPSESSID=session-secret"),
        ],
        "secret-body-never-log-it",
    );

    stop_child(child);
    let log = fs::read_to_string(&debug_log).expect("read debug log");
    fs::remove_dir_all(debug_dir).expect("remove debug temp dir");
    fs::remove_dir_all(docroot).expect("remove temp docroot");
    fs::remove_file(outside).expect("remove outside file");

    assert!(missing.starts_with("HTTP/1.1 404 Not Found"), "{missing}");
    assert!(
        forbidden.starts_with("HTTP/1.1 403 Forbidden"),
        "{forbidden}"
    );
    assert!(
        multipart.starts_with("HTTP/1.1 400 Bad Request"),
        "{multipart}"
    );
    assert!(
        log.contains("E_PHP_SERVER_SCRIPT_RESOLUTION_FAILED"),
        "{log}"
    );
    assert!(log.contains("E_PHP_SERVER_OUTSIDE_DOCUMENT_ROOT"), "{log}");
    assert!(log.contains("E_PHP_REQUEST_BODY_PARSE_FAILED"), "{log}");
    assert!(log.contains("\"method\":\"GET\""), "{log}");
    assert!(
        log.contains("\"uri\":\"/missing.php?token=secret-token\""),
        "{log}"
    );
    assert!(log.contains("\"document_root\""), "{log}");
    assert!(log.contains("\"allowed_roots\""), "{log}");
    assert!(log.contains("\"function_name\":\"resolve_route\""), "{log}");
    assert!(!log.contains("session-secret"), "{log}");
    assert!(!log.contains("secret-body-never-log-it"), "{log}");
}

#[test]
fn server_debug_log_samples_successful_runtime_diagnostics() {
    let docroot = temp_docroot();
    fs::write(
        docroot.join("warn.php"),
        "<?php $items = []; echo $items['missing']; echo 'after';",
    )
    .expect("write warning fixture");
    let debug_dir = temp_docroot();
    let debug_log = debug_dir.join("server-debug.jsonl");
    let debug_log_arg = debug_log.to_string_lossy().to_string();
    let mut child = start_server(
        &docroot,
        &[
            "--debug",
            "--error-format",
            "json",
            "--debug-log",
            &debug_log_arg,
        ],
    );

    let address = read_listening_address(&mut child);
    let response = http_request(&address, "GET", "/warn.php");

    stop_child(child);
    let log = fs::read_to_string(&debug_log).expect("read debug log");
    fs::remove_dir_all(debug_dir).expect("remove debug temp dir");
    fs::remove_dir_all(docroot).expect("remove temp docroot");

    assert!(response.starts_with("HTTP/1.1 200 OK"), "{response}");
    assert!(log.contains("D_PHRUST_SERVER_EXECUTE_END"), "{log}");
    assert!(
        log.contains("runtime_diagnostic_samples"),
        "missing diagnostic samples in {log}"
    );
    assert!(
        log.contains("E_PHP_RUNTIME_UNDEFINED_ARRAY_KEY_WARNING"),
        "{log}"
    );
    assert!(log.contains("missing"), "{log}");
}

#[test]
fn server_debug_off_does_not_emit_request_timeline() {
    let docroot = fixture_docroot("fixtures/server/php");
    let debug_dir = temp_docroot();
    let debug_log = debug_dir.join("server-debug.jsonl");
    let debug_log_arg = debug_log.to_string_lossy().to_string();
    let mut child = start_server(&docroot, &["--debug-log", &debug_log_arg]);

    let address = read_listening_address(&mut child);
    let response = http_request(&address, "GET", "/hello.php");

    stop_child(child);
    let log_exists = debug_log.exists();
    fs::remove_dir_all(debug_dir).expect("remove debug temp dir");

    assert!(response.starts_with("HTTP/1.1 200 OK"), "{response}");
    assert!(
        !log_exists,
        "debug log should not be written unless --debug is set"
    );
}

#[test]
fn server_executes_php_script() {
    let docroot = fixture_docroot("fixtures/server/php");
    let mut child = start_server(&docroot, &[]);

    let address = read_listening_address(&mut child);
    let response = http_request(&address, "GET", "/hello.php");

    stop_child(child);

    assert!(response.starts_with("HTTP/1.1 200 OK"), "{response}");
    assert!(response.ends_with("hello\n"), "{response}");
}

#[test]
fn server_default_engine_profile_matches_baseline_output() {
    let docroot = temp_docroot();
    fs::write(
        docroot.join("profile.php"),
        "<?php $value = 2 + 4; echo $value, \"\\n\";\n",
    )
    .expect("write profile fixture");

    let mut default_child = start_server(&docroot, &["--engine-preset", "default"]);
    let default_address = read_listening_address(&mut default_child);
    let default_response = http_request(&default_address, "GET", "/profile.php");
    stop_child(default_child);

    let mut baseline_child = start_server(&docroot, &["--engine-preset", "baseline"]);
    let baseline_address = read_listening_address(&mut baseline_child);
    let baseline_response = http_request(&baseline_address, "GET", "/profile.php");
    stop_child(baseline_child);
    fs::remove_dir_all(docroot).expect("remove temp docroot");

    assert!(
        default_response.starts_with("HTTP/1.1 200 OK"),
        "{default_response}"
    );
    assert!(
        baseline_response.starts_with("HTTP/1.1 200 OK"),
        "{baseline_response}"
    );
    assert_eq!(
        response_body(&default_response),
        response_body(&baseline_response)
    );
    assert_eq!(response_body(&default_response), "6\n");
}

#[test]
fn server_execution_deadline_returns_timeout_response_and_metric() {
    let docroot = temp_docroot();
    fs::write(
        docroot.join("loop.php"),
        "<?php while (true) { usleep(1000); }\n",
    )
    .expect("write loop fixture");
    let mut child = start_server(&docroot, &["--max-execution-ms", "1"]);

    let address = read_listening_address(&mut child);
    let response = http_request(&address, "GET", "/loop.php");
    let metrics = http_request(&address, "GET", "/__phrust/metrics");

    stop_child(child);
    fs::remove_dir_all(docroot).expect("remove temp docroot");

    assert!(
        response.starts_with("HTTP/1.1 504 Gateway Timeout"),
        "{response}"
    );
    assert_eq!(response_body(&response), "php execution timeout\n");
    assert!(
        metrics.contains("phrust_server_execution_timeouts_total 1"),
        "{metrics}"
    );
}

#[test]
fn server_execution_deadline_leaves_short_script_unaffected() {
    let docroot = temp_docroot();
    fs::write(docroot.join("short.php"), "<?php echo \"short\\n\";\n")
        .expect("write short fixture");
    let mut child = start_server(&docroot, &["--max-execution-ms", "1000"]);

    let address = read_listening_address(&mut child);
    let response = http_request(&address, "GET", "/short.php");

    stop_child(child);
    fs::remove_dir_all(docroot).expect("remove temp docroot");

    assert!(response.starts_with("HTTP/1.1 200 OK"), "{response}");
    assert_eq!(response_body(&response), "short\n");
}

#[test]
fn server_reports_disabled_execution_deadline_metric() {
    let docroot = temp_docroot();
    fs::write(docroot.join("short.php"), "<?php echo \"short\\n\";\n")
        .expect("write short fixture");
    let mut child = start_server(&docroot, &["--disable-execution-deadline"]);

    let address = read_listening_address(&mut child);
    let response = http_request(&address, "GET", "/short.php");
    let metrics = http_request(&address, "GET", "/__phrust/metrics");

    stop_child(child);
    fs::remove_dir_all(docroot).expect("remove temp docroot");

    assert!(response.starts_with("HTTP/1.1 200 OK"), "{response}");
    assert!(
        metrics.contains("phrust_server_execution_deadline_disabled_total 1"),
        "{metrics}"
    );
}

#[test]
fn server_applies_php_response_header() {
    let docroot = fixture_docroot("fixtures/server/php");
    let mut child = start_server(&docroot, &[]);

    let address = read_listening_address(&mut child);
    let response = http_request(&address, "GET", "/header.php");

    stop_child(child);

    assert!(response.starts_with("HTTP/1.1 200 OK"), "{response}");
    assert_response_contains_header(&response, "x-test", "yes");
    assert!(response.ends_with("ok\n"), "{response}");
}

#[test]
fn server_replaces_php_response_header_by_default() {
    let docroot = fixture_docroot("fixtures/server/php");
    let mut child = start_server(&docroot, &[]);

    let address = read_listening_address(&mut child);
    let response = http_request(&address, "GET", "/header_replace.php");

    stop_child(child);

    assert!(response.starts_with("HTTP/1.1 200 OK"), "{response}");
    assert_response_contains_header(&response, "x-test", "two");
    assert_response_lacks_header(&response, "x-test", "one");
}

#[test]
fn server_applies_php_response_status() {
    let docroot = fixture_docroot("fixtures/server/php");
    let mut child = start_server(&docroot, &[]);

    let address = read_listening_address(&mut child);
    let response = http_request(&address, "GET", "/status.php");

    stop_child(child);

    assert!(response.starts_with("HTTP/1.1 201 Created"), "{response}");
    assert!(response.ends_with("created\n"), "{response}");
}

#[test]
fn server_exposes_headers_list_builtin() {
    let docroot = fixture_docroot("fixtures/server/php");
    let mut child = start_server(&docroot, &[]);

    let address = read_listening_address(&mut child);
    let response = http_request(&address, "GET", "/headers_list.php");

    stop_child(child);

    assert!(response.starts_with("HTTP/1.1 200 OK"), "{response}");
    assert_response_contains_header(&response, "x-test", "yes");
    assert!(response.ends_with("X-Test: yes\n"), "{response}");
}

#[test]
fn server_preserves_multiple_set_cookie_headers() {
    let docroot = fixture_docroot("fixtures/server/php");
    let mut child = start_server(&docroot, &[]);

    let address = read_listening_address(&mut child);
    let response = http_request(&address, "GET", "/cookie_builtin.php");

    stop_child(child);

    assert!(response.starts_with("HTTP/1.1 200 OK"), "{response}");
    assert_response_contains_header(
        &response,
        "set-cookie",
        "login=hello%20world; Path=/; Secure; HttpOnly; SameSite=Lax",
    );
    assert_response_contains_header(&response, "set-cookie", "raw=a=b; Path=/raw");
    assert_eq!(
        response_header_count(&response, "set-cookie"),
        2,
        "{response}"
    );
    assert_eq!(
        response_body(&response),
        "Set-Cookie: login=hello%20world; Path=/; Secure; HttpOnly; SameSite=Lax\nSet-Cookie: raw=a=b; Path=/raw\n",
        "{response}"
    );
}

#[test]
fn incoming_cookie_header_seeds_cookie_without_response_cookie() {
    let docroot = fixture_docroot("fixtures/server/php");
    let mut child = start_server(&docroot, &[]);

    let address = read_listening_address(&mut child);
    let response = http_request_with_headers(
        &address,
        "GET",
        "/incoming_cookie.php",
        &[("Cookie", "theme=dark")],
        "",
    );

    stop_child(child);

    assert!(response.starts_with("HTTP/1.1 200 OK"), "{response}");
    assert_eq!(response_body(&response), "dark\n", "{response}");
    assert_eq!(
        response_header_count(&response, "set-cookie"),
        0,
        "{response}"
    );
}

#[test]
fn non_session_request_with_session_cookie_does_not_load_session_store() {
    let docroot = fixture_docroot("fixtures/server/php");
    let mut child = start_server(&docroot, &[]);

    let address = read_listening_address(&mut child);
    let response = http_request_with_headers(
        &address,
        "GET",
        "/hello.php",
        &[("Cookie", "PHPSESSID=session-secret")],
        "",
    );
    let metrics = http_request(&address, "GET", "/__phrust/metrics");

    stop_child(child);

    assert!(response.starts_with("HTTP/1.1 200 OK"), "{response}");
    assert!(
        metrics.contains("phrust_server_session_seed_attempts_total 1\n"),
        "{metrics}"
    );
    assert!(
        metrics.contains("phrust_server_session_store_loads_total 0\n"),
        "{metrics}"
    );
    assert!(
        metrics.contains("phrust_server_session_lazy_loads_total 0\n"),
        "{metrics}"
    );
    assert!(
        metrics.contains("phrust_server_session_finalize_skipped_inactive_total 1\n"),
        "{metrics}"
    );
    assert!(
        metrics.contains("phrust_server_request_headers_seen_total 3\n"),
        "{metrics}"
    );
    assert!(
        metrics.contains("phrust_server_request_headers_materialized_total 2\n"),
        "{metrics}"
    );
    assert!(
        metrics.contains("phrust_server_request_headers_skipped_direct_total 1\n"),
        "{metrics}"
    );
}

#[test]
fn server_persists_web_sessions_across_requests() {
    let docroot = fixture_docroot("fixtures/server/php");
    let session_dir = temp_docroot();
    let session_arg = session_dir.to_string_lossy().to_string();
    let mut child = start_server(&docroot, &["--session-save-path", &session_arg]);

    let address = read_listening_address(&mut child);
    let first = http_request(&address, "GET", "/session_counter.php");
    assert!(first.starts_with("HTTP/1.1 200 OK"), "{first}");
    let set_cookie = response_header_values(&first, "set-cookie");
    assert_eq!(set_cookie.len(), 1, "{first}");
    assert!(set_cookie[0].ends_with("; Path=/; HttpOnly"), "{first}");
    let cookie_pair = set_cookie[0]
        .split_once(';')
        .map_or(set_cookie[0], |(pair, _)| pair)
        .to_string();
    let session_id = cookie_pair
        .strip_prefix("PHPSESSID=")
        .expect("session cookie name")
        .to_string();
    assert_eq!(
        response_body(&first),
        format!("id={session_id}\nn=1\nstatus=2\n")
    );

    let second = http_request_with_headers(
        &address,
        "GET",
        "/session_counter.php",
        &[("Cookie", &cookie_pair)],
        "",
    );
    assert!(second.starts_with("HTTP/1.1 200 OK"), "{second}");
    assert_eq!(
        response_body(&second),
        format!("id={session_id}\nn=2\nstatus=2\n")
    );
    assert_eq!(response_header_count(&second, "set-cookie"), 0, "{second}");

    let destroy = http_request_with_headers(
        &address,
        "GET",
        "/session_destroy.php",
        &[("Cookie", &cookie_pair)],
        "",
    );
    assert!(destroy.starts_with("HTTP/1.1 200 OK"), "{destroy}");
    assert_eq!(
        response_body(&destroy),
        format!("id={session_id}\ndestroyed=yes\n")
    );
    assert!(
        !session_dir.join(format!("sess_{session_id}")).exists(),
        "destroyed session file should be removed"
    );

    let after_destroy = http_request_with_headers(
        &address,
        "GET",
        "/session_counter.php",
        &[("Cookie", &cookie_pair)],
        "",
    );

    stop_child(child);
    fs::remove_dir_all(session_dir).expect("remove session temp dir");

    assert!(
        after_destroy.starts_with("HTTP/1.1 200 OK"),
        "{after_destroy}"
    );
    assert_eq!(
        response_body(&after_destroy),
        format!("id={session_id}\nn=1\nstatus=2\n")
    );
}

#[test]
fn server_reports_headers_not_sent_during_php_execution() {
    let docroot = fixture_docroot("fixtures/server/php");
    let mut child = start_server(&docroot, &[]);

    let address = read_listening_address(&mut child);
    let response = http_request(&address, "GET", "/headers_sent.php");

    stop_child(child);

    assert!(response.starts_with("HTTP/1.1 200 OK"), "{response}");
    assert!(response.ends_with("not-sent\n"), "{response}");
}

#[test]
fn server_rejects_response_splitting_header() {
    let docroot = fixture_docroot("fixtures/server/php");
    let mut child = start_server(&docroot, &[]);

    let address = read_listening_address(&mut child);
    let response = http_request(&address, "GET", "/invalid_header.php");

    stop_child(child);

    assert!(response.starts_with("HTTP/1.1 200 OK"), "{response}");
    assert_response_lacks_header(&response, "x-evil", "yes");
    assert!(response.ends_with("ok\n"), "{response}");
}

#[test]
fn server_does_not_share_php_response_headers_between_requests() {
    let docroot = fixture_docroot("fixtures/server/php");
    let mut child = start_server(&docroot, &[]);

    let address = read_listening_address(&mut child);
    let first_response = http_request(&address, "GET", "/header.php");
    let second_response = http_request(&address, "GET", "/hello.php");

    stop_child(child);

    assert_response_contains_header(&first_response, "x-test", "yes");
    assert!(
        second_response.starts_with("HTTP/1.1 200 OK"),
        "{second_response}"
    );
    assert_response_lacks_header(&second_response, "x-test", "yes");
    assert!(second_response.ends_with("hello\n"), "{second_response}");
}

#[test]
fn server_exposes_query_superglobal() {
    let docroot = fixture_docroot("fixtures/server/php");
    let mut child = start_server(&docroot, &[]);

    let address = read_listening_address(&mut child);
    let response = http_request(&address, "GET", "/query.php?name=phrust");

    stop_child(child);

    assert!(response.starts_with("HTTP/1.1 200 OK"), "{response}");
    assert!(response.ends_with("phrust\n"), "{response}");
}

#[test]
fn server_exposes_post_superglobal() {
    let docroot = fixture_docroot("fixtures/server/php");
    let mut child = start_server(&docroot, &[]);

    let address = read_listening_address(&mut child);
    let response = http_request_with_body(
        &address,
        "POST",
        "/post.php",
        "application/x-www-form-urlencoded",
        "name=phrust",
    );

    stop_child(child);

    assert!(response.starts_with("HTTP/1.1 200 OK"), "{response}");
    assert!(response.ends_with("phrust\n"), "{response}");
}

#[test]
fn server_exposes_selected_server_superglobals() {
    let docroot = fixture_docroot("fixtures/server/php");
    let mut child = start_server(&docroot, &[]);

    let address = read_listening_address(&mut child);
    let response = http_request(&address, "GET", "/server.php?name=phrust");

    stop_child(child);

    assert!(response.starts_with("HTTP/1.1 200 OK"), "{response}");
    assert!(
        response.ends_with("GET|/server.php|/server.php?name=phrust\n"),
        "{response}"
    );
}

#[test]
fn server_executes_front_controller() {
    let docroot = fixture_docroot("fixtures/server/front/public");
    let mut child = start_server(&docroot, &["--front-controller", "index.php"]);

    let address = read_listening_address(&mut child);
    let response = http_request(&address, "GET", "/users/123?name=phrust");

    stop_child(child);

    assert!(response.starts_with("HTTP/1.1 200 OK"), "{response}");
    assert!(
        response.ends_with("/index.php|/users/123|phrust\n"),
        "{response}"
    );
}

#[test]
fn server_basic_app_fixture_outputs_match_exactly() {
    let docroot = fixture_docroot("fixtures/server/apps/basic/public");
    let mut child = start_server(&docroot, &[]);

    let address = read_listening_address(&mut child);
    let echo = http_request(&address, "GET", "/echo.php");
    let static_file = http_request(&address, "GET", "/static.txt");
    let query = http_request(&address, "GET", "/query.php?name=phrust");
    let form = http_request_with_body(
        &address,
        "POST",
        "/form.php",
        "application/x-www-form-urlencoded",
        "name=phrust",
    );
    let cookie = http_request_with_headers(
        &address,
        "GET",
        "/cookie.php",
        &[("Cookie", "sid=abc; theme=dark")],
        "",
    );
    let server = http_request(&address, "GET", "/server.php?name=phrust");
    let include = http_request(&address, "GET", "/include.php");
    let header = http_request(&address, "GET", "/header.php");

    stop_child(child);

    assert!(echo.starts_with("HTTP/1.1 200 OK"), "{echo}");
    assert_eq!(response_body(&echo), "basic echo\n");
    assert!(static_file.starts_with("HTTP/1.1 200 OK"), "{static_file}");
    assert_eq!(response_body(&static_file), "basic static fixture\n");
    assert!(query.starts_with("HTTP/1.1 200 OK"), "{query}");
    assert_eq!(response_body(&query), "query=phrust\n");
    assert!(form.starts_with("HTTP/1.1 200 OK"), "{form}");
    assert_eq!(response_body(&form), "form=phrust\n");
    assert!(cookie.starts_with("HTTP/1.1 200 OK"), "{cookie}");
    assert_eq!(response_body(&cookie), "cookie=dark\n");
    assert!(server.starts_with("HTTP/1.1 200 OK"), "{server}");
    assert_eq!(
        response_body(&server),
        format!(
            "server=GET|/server.php?name=phrust|/server.php|/server.php|{}|{}\n",
            docroot.join("server.php").to_string_lossy(),
            docroot.to_string_lossy()
        )
    );
    assert!(include.starts_with("HTTP/1.1 200 OK"), "{include}");
    assert_eq!(response_body(&include), "include=from required file\n");
    assert!(header.starts_with("HTTP/1.1 202 Accepted"), "{header}");
    assert_response_contains_header(&header, "x-app-fixture", "basic");
    assert_eq!(response_body(&header), "accepted\n");
}

#[test]
fn server_reuses_include_cache_across_requests() {
    let docroot = fixture_docroot("fixtures/server/apps/compat/public");
    let mut child = start_server(&docroot, &[]);

    let address = read_listening_address(&mut child);
    let first = http_request(&address, "GET", "/include-entry.php");
    let second = http_request(&address, "GET", "/include-entry.php");
    let metrics = http_request(&address, "GET", "/__phrust/metrics");

    stop_child(child);

    assert!(first.starts_with("HTTP/1.1 200 OK"), "{first}");
    assert_eq!(response_body(&first), "compat include helper\n");
    assert!(second.starts_with("HTTP/1.1 200 OK"), "{second}");
    assert_eq!(response_body(&second), "compat include helper\n");
    assert!(
        metrics.contains("phrust_server_include_resolution_misses_total 1"),
        "{metrics}"
    );
    assert!(
        metrics.contains("phrust_server_include_resolution_hits_total 1"),
        "{metrics}"
    );
    assert!(
        metrics.contains("phrust_server_include_compile_misses_total 1"),
        "{metrics}"
    );
    assert!(
        metrics.contains("phrust_server_include_compile_hits_total 1"),
        "{metrics}"
    );
}

#[test]
fn server_front_controller_app_fixture_dispatches_from_path_info() {
    let docroot = fixture_docroot("fixtures/server/apps/front-controller/public");
    let mut child = start_server(&docroot, &["--front-controller", "index.php"]);

    let address = read_listening_address(&mut child);
    let user = http_request(&address, "GET", "/users/42?name=phrust");
    let missing = http_request(&address, "GET", "/missing");

    stop_child(child);

    assert!(user.starts_with("HTTP/1.1 200 OK"), "{user}");
    assert_eq!(
        response_body(&user),
        "front=user|/index.php|/index.php/users/42|/users/42|/users/42?name=phrust\n"
    );
    assert!(missing.starts_with("HTTP/1.1 404 Not Found"), "{missing}");
    assert_eq!(response_body(&missing), "front=missing|/missing\n");
}

#[test]
fn server_wordpress_like_entrypoints_map_request_environment() {
    let docroot = fixture_docroot("fixtures/server/apps/wordpress-like/public");
    let mut child = start_server(&docroot, &["--front-controller", "index.php"]);

    let address = read_listening_address(&mut child);
    let home = http_request(&address, "GET", "/");
    let index = http_request(&address, "GET", "/index.php?preview=1");
    let install = http_request(&address, "GET", "/wp-admin/install.php?step=2");
    let admin = http_request(&address, "GET", "/wp-admin/");
    let pretty = http_request(&address, "GET", "/category/news?paged=2");
    let encoded = http_request(&address, "GET", "/index.php/wp%20admin/setup?ok=1");

    stop_child(child);

    assert!(home.starts_with("HTTP/1.1 200 OK"), "{home}");
    assert_eq!(
        response_body(&home),
        "wordpress-like|alpha=1|route=home|class=yes|function=yes|cookie=none|post=42:Hello Hotpath:7|post=43:Cache Warm:11|post=44:Array Lookup:13|beta=1\n"
    );
    assert!(index.starts_with("HTTP/1.1 200 OK"), "{index}");
    assert_eq!(
        response_body(&index),
        "wordpress-like|alpha=1|route=home|class=yes|function=yes|cookie=none|post=42:Hello Hotpath:7|post=43:Cache Warm:11|post=44:Array Lookup:13|beta=1\n"
    );
    assert!(install.starts_with("HTTP/1.1 200 OK"), "{install}");
    assert_eq!(
        response_body(&install),
        "install|/wp-admin/install.php?step=2|/wp-admin/install.php|/wp-admin/install.php|/wp-admin/install.php||step=2|wordpress-like-loader\n"
    );
    assert!(admin.starts_with("HTTP/1.1 200 OK"), "{admin}");
    assert_eq!(
        response_body(&admin),
        "admin-index|/wp-admin/|/wp-admin/index.php|/wp-admin/index.php|\n"
    );
    assert!(pretty.starts_with("HTTP/1.1 200 OK"), "{pretty}");
    assert_eq!(
        response_body(&pretty),
        "wordpress-like|alpha=1|route=archive|class=yes|function=yes|cookie=none|post=42:Hello Hotpath:7|post=43:Cache Warm:11|post=44:Array Lookup:13|beta=1\n"
    );
    assert!(encoded.starts_with("HTTP/1.1 200 OK"), "{encoded}");
    assert_eq!(
        response_body(&encoded),
        "wordpress-like|alpha=1|route=archive|class=yes|function=yes|cookie=none|post=42:Hello Hotpath:7|post=43:Cache Warm:11|post=44:Array Lookup:13|beta=1\n"
    );
}

#[test]
fn server_synthetic_plugin_theme_fixture_runs() {
    let docroot = fixture_docroot("fixtures/integration/wp_plugin_theme_synthetic/public");
    let mut child = start_server(&docroot, &["--front-controller", "index.php"]);

    let address = read_listening_address(&mut child);
    let page = http_request(&address, "GET", "/?name=demo");
    let redirect = http_request(&address, "GET", "/?redirect=1");
    let upload_body = "--BOUNDARY\r\nContent-Disposition: form-data; name=\"package\"; filename=\"sample.txt\"\r\nContent-Type: text/plain\r\n\r\nsynthetic package payload\n\r\n--BOUNDARY--";
    let upload = http_request_with_body(
        &address,
        "POST",
        "/?upload=1",
        "multipart/form-data; boundary=BOUNDARY",
        upload_body,
    );

    stop_child(child);

    assert!(page.starts_with("HTTP/1.1 200 OK"), "{page}");
    assert_response_contains_header(&page, "x-synthetic-fixture", "ok");
    assert_response_contains_header(
        &page,
        "set-cookie",
        "synthetic_demo=enabled; Path=/; SameSite=Lax",
    );
    assert_eq!(
        response_body(&page),
        "template=demo\nplugin=active\npackage_size=14\nupload=none\n"
    );
    assert!(redirect.starts_with("HTTP/1.1 302 Found"), "{redirect}");
    assert_response_contains_header(&redirect, "location", "/activated");
    assert_response_contains_header(
        &redirect,
        "set-cookie",
        "synthetic_demo=redirect; Path=/; SameSite=Lax",
    );
    assert!(upload.starts_with("HTTP/1.1 200 OK"), "{upload}");
    assert_eq!(
        response_body(&upload),
        "template=synthetic\nplugin=active\npackage_size=14\nupload=moved\nupload_size=26\n"
    );
}

#[test]
fn successful_php_redirect_without_stdout_has_empty_body() {
    let docroot = temp_docroot();
    fs::write(
        docroot.join("redirect.php"),
        "<?php header('Location: /next', true, 302); exit;",
    )
    .expect("write redirect fixture");
    let mut child = start_server(&docroot, &[]);

    let address = read_listening_address(&mut child);
    let redirect = http_request(&address, "GET", "/redirect.php");

    stop_child(child);
    fs::remove_dir_all(docroot).expect("remove temp docroot");

    assert!(redirect.starts_with("HTTP/1.1 302 Found"), "{redirect}");
    assert_response_contains_header(&redirect, "location", "/next");
    assert_response_contains_header(&redirect, "content-length", "0");
    assert_eq!(response_body(&redirect), "");
}

#[cfg(unix)]
#[test]
fn server_rejects_symlink_escape_from_docroot() {
    let docroot = temp_docroot();
    fs::write(docroot.join("index.php"), "<?php echo 'ok';").expect("write index");
    let outside = docroot.with_extension("outside");
    fs::write(&outside, "secret").expect("write outside file");
    std::os::unix::fs::symlink(&outside, docroot.join("link.php")).expect("create symlink");
    let mut child = start_server(&docroot, &[]);

    let address = read_listening_address(&mut child);
    let response = http_request(&address, "GET", "/link.php");

    stop_child(child);
    fs::remove_dir_all(docroot).expect("remove temp docroot");
    fs::remove_file(outside).expect("remove outside file");

    assert!(response.starts_with("HTTP/1.1 403 Forbidden"), "{response}");
    assert_eq!(response_body(&response), "forbidden\n");
}

#[test]
fn server_returns_404_for_missing_php_script() {
    let docroot = fixture_docroot("fixtures/server/php");
    let mut child = start_server(&docroot, &[]);

    let address = read_listening_address(&mut child);
    let response = http_request(&address, "GET", "/missing.php");

    stop_child(child);

    assert!(response.starts_with("HTTP/1.1 404 Not Found"), "{response}");
}

#[test]
fn server_rejects_request_body_over_limit() {
    let docroot = fixture_docroot("fixtures/server/php");
    let mut child = start_server(&docroot, &["--max-body-bytes", "4"]);

    let address = read_listening_address(&mut child);
    let response = http_request_with_body(
        &address,
        "POST",
        "/post.php",
        "application/x-www-form-urlencoded",
        "name=phrust",
    );

    stop_child(child);

    assert!(
        response.starts_with("HTTP/1.1 413 Payload Too Large"),
        "{response}"
    );
}

#[test]
fn server_exposes_multipart_post_and_files_superglobals() {
    let docroot = fixture_docroot("fixtures/server/apps/compat/public");
    let upload_temp_dir = temp_docroot();
    let upload_temp_arg = upload_temp_dir.to_string_lossy().to_string();
    let mut child = start_server(&docroot, &["--upload-temp-dir", &upload_temp_arg]);

    let address = read_listening_address(&mut child);
    let body = "--BOUNDARY\r\nContent-Disposition: form-data; name=\"title\"\r\n\r\nHello\r\n--BOUNDARY\r\nContent-Disposition: form-data; name=\"avatar\"; filename=\"../me.png\"\r\nContent-Type: image/png\r\n\r\nPNGDATA\r\n--BOUNDARY--";
    let response = http_request_with_body(
        &address,
        "POST",
        "/upload.php",
        "multipart/form-data; boundary=BOUNDARY",
        body,
    );

    stop_child(child);

    assert!(response.starts_with("HTTP/1.1 200 OK"), "{response}");
    assert_eq!(
        response_body(&response),
        "title=Hello\nname=me.png\ntype=image/png\nsize=7\nerror=0\nuploaded=yes\nmoved=yes\ncontent=PNGDATA\nuploaded_after=no\n"
    );
    let moved_upload = docroot.join("moved-upload.txt");
    assert_eq!(fs::read_to_string(&moved_upload).unwrap(), "PNGDATA");
    fs::remove_file(moved_upload).expect("remove moved upload");
    assert_eq!(fs::read_dir(&upload_temp_dir).unwrap().count(), 0);
    fs::remove_dir_all(upload_temp_dir).expect("remove upload temp dir");
}

#[test]
fn server_rejects_malformed_multipart() {
    let docroot = fixture_docroot("fixtures/server/apps/compat/public");
    let mut child = start_server(&docroot, &[]);

    let address = read_listening_address(&mut child);
    let response = http_request_with_body(
        &address,
        "POST",
        "/upload.php",
        "multipart/form-data",
        "not multipart",
    );

    stop_child(child);

    assert!(
        response.starts_with("HTTP/1.1 400 Bad Request"),
        "{response}"
    );
    assert_eq!(response_body(&response), "bad multipart request\n");
}

#[test]
fn server_rejects_upload_file_over_limit() {
    let docroot = fixture_docroot("fixtures/server/apps/compat/public");
    let mut child = start_server(&docroot, &["--max-upload-file-bytes", "4"]);

    let address = read_listening_address(&mut child);
    let body = "--BOUNDARY\r\nContent-Disposition: form-data; name=\"avatar\"; filename=\"me.png\"\r\nContent-Type: image/png\r\n\r\nPNGDATA\r\n--BOUNDARY--";
    let response = http_request_with_body(
        &address,
        "POST",
        "/upload.php",
        "multipart/form-data; boundary=BOUNDARY",
        body,
    );

    stop_child(child);

    assert!(
        response.starts_with("HTTP/1.1 413 Payload Too Large"),
        "{response}"
    );
    assert_eq!(response_body(&response), "upload rejected\n");
}

#[test]
fn server_handles_default_two_hundred_concurrent_requests() {
    let docroot = temp_docroot();
    fs::write(docroot.join("static.txt"), "ok\n").expect("write static fixture");
    let mut child = start_server(&docroot, &[]);

    let address = read_listening_address(&mut child);
    let start = Arc::new(Barrier::new(201));
    let mut handles = Vec::with_capacity(200);
    for _ in 0..200 {
        let address = address.clone();
        let start = Arc::clone(&start);
        handles.push(std::thread::spawn(move || {
            start.wait();
            http_request(&address, "GET", "/static.txt")
        }));
    }
    start.wait();
    let responses = handles
        .into_iter()
        .map(|handle| handle.join().expect("join request thread"))
        .collect::<Vec<_>>();

    stop_child(child);
    fs::remove_dir_all(docroot).expect("remove temp docroot");

    for response in responses {
        assert!(response.starts_with("HTTP/1.1 200 OK"), "{response}");
        assert_eq!(response_body(&response), "ok\n");
    }
}

#[test]
fn server_waits_for_in_flight_capacity_before_overload() {
    let docroot = fixture_docroot("fixtures/server/php");
    let mut child = start_server(
        &docroot,
        &["--max-in-flight", "1", "--request-timeout-ms", "5000"],
    );

    let address = read_listening_address(&mut child);
    let mut held_stream = TcpStream::connect(&address).expect("connect held request");
    held_stream
        .write_all(
            b"POST /post.php HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/x-www-form-urlencoded\r\nContent-Length: 11\r\nConnection: close\r\n\r\n",
        )
        .expect("write held request headers");

    let queued_address = address.clone();
    let queued = std::thread::spawn(move || http_request(&queued_address, "GET", "/hello.php"));
    std::thread::sleep(Duration::from_millis(100));
    held_stream
        .write_all(b"name=queued")
        .expect("finish held request body");
    held_stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .expect("set held read timeout");
    let mut held_response = String::new();
    held_stream
        .read_to_string(&mut held_response)
        .expect("read held response");
    let queued_response = queued.join().expect("join queued request");

    stop_child(child);

    assert!(
        held_response.starts_with("HTTP/1.1 200 OK"),
        "{held_response}"
    );
    assert!(held_response.ends_with("queued\n"), "{held_response}");
    assert!(
        queued_response.starts_with("HTTP/1.1 200 OK"),
        "{queued_response}"
    );
    assert!(queued_response.ends_with("hello\n"), "{queued_response}");
}

#[test]
fn server_runs_session_requests_without_global_execution_lock() {
    let docroot = temp_docroot();
    fs::write(
        docroot.join("session_slow.php"),
        "<?php\nsession_start();\nusleep(400000);\necho \"done\\n\";\n",
    )
    .expect("write slow session fixture");
    let mut child = start_server(
        &docroot,
        &["--max-in-flight", "4", "--max-execution-ms", "5000"],
    );

    let address = read_listening_address(&mut child);
    let warmup = http_request(&address, "GET", "/session_slow.php");
    assert!(warmup.starts_with("HTTP/1.1 200 OK"), "{warmup}");

    let start = Arc::new(Barrier::new(4));
    let started = Instant::now();
    let mut handles = Vec::with_capacity(3);
    for _ in 0..3 {
        let address = address.clone();
        let start = Arc::clone(&start);
        handles.push(std::thread::spawn(move || {
            start.wait();
            http_request(&address, "GET", "/session_slow.php")
        }));
    }
    start.wait();
    let responses = handles
        .into_iter()
        .map(|handle| handle.join().expect("join session request thread"))
        .collect::<Vec<_>>();
    let elapsed = started.elapsed();

    stop_child(child);
    fs::remove_dir_all(docroot).expect("remove temp docroot");

    for response in responses {
        assert!(response.starts_with("HTTP/1.1 200 OK"), "{response}");
        assert!(response.ends_with("done\n"), "{response}");
    }
    assert!(
        elapsed < Duration::from_millis(1_000),
        "session requests were serialized: elapsed={elapsed:?}"
    );
}

#[test]
fn server_returns_503_when_max_in_flight_wait_expires() {
    let docroot = fixture_docroot("fixtures/server/php");
    let mut child = start_server(
        &docroot,
        &["--max-in-flight", "1", "--request-timeout-ms", "5000"],
    );

    let address = read_listening_address(&mut child);
    let mut held_stream = TcpStream::connect(&address).expect("connect held request");
    held_stream
        .write_all(
            b"POST /post.php HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/x-www-form-urlencoded\r\nContent-Length: 11\r\nConnection: close\r\n\r\n",
        )
        .expect("write held request headers");
    std::thread::sleep(Duration::from_millis(100));

    let started = Instant::now();
    let response = http_request(&address, "GET", "/hello.php");
    let elapsed = started.elapsed();

    drop(held_stream);
    stop_child(child);

    assert!(
        response.starts_with("HTTP/1.1 503 Service Unavailable"),
        "{response}"
    );
    assert_response_contains_header(&response, "retry-after", "1");
    assert!(response.ends_with("server overloaded\n"), "{response}");
    assert!(
        elapsed >= Duration::from_millis(450),
        "overload response did not wait for capacity: elapsed={elapsed:?}"
    );
}

#[test]
fn server_shutdown_signal_does_not_panic() {
    let docroot = temp_docroot();
    let mut child = start_server(&docroot, &[]);

    let address = read_listening_address(&mut child);
    let response = http_get(&address, "/healthz");
    assert!(response.starts_with("HTTP/1.1 200 OK"), "{response}");

    send_sigint(&child);
    let status = wait_for_exit(&mut child, Duration::from_secs(5));
    fs::remove_dir_all(docroot).expect("remove temp docroot");

    assert!(status.success(), "server exited with {status}");
}

fn start_server(docroot: &std::path::Path, extra_args: &[&str]) -> Child {
    let mut command = Proc::new(env!("CARGO_BIN_EXE_phrust-server"));
    command
        .args(["--listen", "127.0.0.1:0", "--docroot"])
        .arg(docroot)
        .args(extra_args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    command.spawn().expect("spawn phrust-server")
}

fn temp_docroot() -> std::path::PathBuf {
    for attempt in 0..100 {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "phrust-server-health-{}-{unique}-{attempt}",
            std::process::id()
        ));
        match fs::create_dir(&path) {
            Ok(()) => return path,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => panic!("create temp docroot: {error}"),
        }
    }
    panic!("create unique temp docroot");
}

fn fixture_docroot(path: &str) -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(path)
        .canonicalize()
        .expect("fixture docroot")
}

fn read_listening_address(child: &mut Child) -> String {
    let stdout = child.stdout.take().expect("child stdout");
    let mut reader = BufReader::new(stdout);
    let mut line = String::new();
    reader
        .read_line(&mut line)
        .expect("read listening line from server");
    line.strip_prefix("listening http://")
        .expect("listening line prefix")
        .trim()
        .to_string()
}

fn http_get(address: &str, path: &str) -> String {
    http_request(address, "GET", path)
}

fn http_request(address: &str, method: &str, path: &str) -> String {
    http_request_with_headers(address, method, path, &[], "")
}

fn http_request_with_body(
    address: &str,
    method: &str,
    path: &str,
    content_type: &str,
    body: &str,
) -> String {
    let content_length = body.len().to_string();
    http_request_with_headers(
        address,
        method,
        path,
        &[
            ("Content-Type", content_type),
            ("Content-Length", content_length.as_str()),
        ],
        body,
    )
}

fn http_request_with_headers(
    address: &str,
    method: &str,
    path: &str,
    headers: &[(&str, &str)],
    body: &str,
) -> String {
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    loop {
        match TcpStream::connect(address) {
            Ok(mut stream) => {
                let mut request =
                    format!("{method} {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n");
                for (name, value) in headers {
                    request.push_str(name);
                    request.push_str(": ");
                    request.push_str(value);
                    request.push_str("\r\n");
                }
                request.push_str("\r\n");
                request.push_str(body);
                stream.write_all(request.as_bytes()).expect("write request");
                stream
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .expect("set read timeout");
                let mut response = String::new();
                stream.read_to_string(&mut response).expect("read response");
                return response;
            }
            Err(error) if std::time::Instant::now() < deadline => {
                let _ = error;
                std::thread::sleep(Duration::from_millis(20));
            }
            Err(error) => panic!("connect to server: {error}"),
        }
    }
}

fn stop_child(mut child: Child) {
    let _ = child.kill();
    let _ = child.wait();
}

fn send_sigint(child: &Child) {
    let status = Proc::new("kill")
        .args(["-INT", &child.id().to_string()])
        .status()
        .expect("send SIGINT");
    assert!(status.success(), "kill -INT failed with {status}");
}

fn wait_for_exit(child: &mut Child, timeout: Duration) -> std::process::ExitStatus {
    let deadline = std::time::Instant::now() + timeout;
    loop {
        if let Some(status) = child.try_wait().expect("poll child exit") {
            return status;
        }
        if std::time::Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("server did not exit within {timeout:?}");
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn assert_response_contains_header(response: &str, name: &str, value: &str) {
    assert!(
        response_headers(response).any(|line| header_line_matches(line, name, value)),
        "missing header {name}: {value}\n{response}"
    );
}

fn assert_response_lacks_header(response: &str, name: &str, value: &str) {
    assert!(
        !response_headers(response).any(|line| header_line_matches(line, name, value)),
        "unexpected header {name}: {value}\n{response}"
    );
}

fn response_headers(response: &str) -> impl Iterator<Item = &str> {
    response
        .split_once("\r\n\r\n")
        .map_or(response, |(headers, _)| headers)
        .lines()
        .skip(1)
}

fn response_header_count(response: &str, expected_name: &str) -> usize {
    response_headers(response)
        .filter(|line| {
            line.split_once(':')
                .is_some_and(|(name, _)| name.trim().eq_ignore_ascii_case(expected_name))
        })
        .count()
}

fn response_header_values<'a>(response: &'a str, expected_name: &str) -> Vec<&'a str> {
    response_headers(response)
        .filter_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.trim()
                .eq_ignore_ascii_case(expected_name)
                .then_some(value.trim())
        })
        .collect()
}

fn response_body(response: &str) -> &str {
    response.split_once("\r\n\r\n").map_or("", |(_, body)| body)
}

fn header_line_matches(line: &str, expected_name: &str, expected_value: &str) -> bool {
    let Some((name, value)) = line.split_once(':') else {
        return false;
    };
    name.trim().eq_ignore_ascii_case(expected_name) && value.trim() == expected_value
}
