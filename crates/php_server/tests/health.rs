use std::{
    fs,
    io::{BufRead, BufReader, Read, Write},
    net::TcpStream,
    process::{Child, Command as Proc, Stdio},
    time::{Duration, SystemTime, UNIX_EPOCH},
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
fn server_returns_503_when_max_in_flight_is_exhausted() {
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

    let response = http_request(&address, "GET", "/hello.php");

    drop(held_stream);
    stop_child(child);

    assert!(
        response.starts_with("HTTP/1.1 503 Service Unavailable"),
        "{response}"
    );
    assert_response_contains_header(&response, "retry-after", "1");
    assert!(response.ends_with("server overloaded\n"), "{response}");
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
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time")
        .as_nanos();
    let path = std::env::temp_dir().join(format!(
        "phrust-server-health-{}-{unique}",
        std::process::id()
    ));
    fs::create_dir(&path).expect("create temp docroot");
    path
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

fn response_body(response: &str) -> &str {
    response.split_once("\r\n\r\n").map_or("", |(_, body)| body)
}

fn header_line_matches(line: &str, expected_name: &str, expected_value: &str) -> bool {
    let Some((name, value)) = line.split_once(':') else {
        return false;
    };
    name.trim().eq_ignore_ascii_case(expected_name) && value.trim() == expected_value
}
