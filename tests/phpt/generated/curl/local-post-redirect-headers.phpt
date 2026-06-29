--TEST--
curl: local POST fields, headers, redirects, and response headers
--DESCRIPTION--
Generated coverage for the selected deterministic cURL transport slice.
The configured URL must point at a local test server that 307-redirects
the first POST request to a final endpoint and echoes a 201 response.
--SKIPIF--
<?php
if (!extension_loaded("curl")) {
    die("skip curl extension is not loaded");
}
if (getenv("PHRUST_NET_TESTS") !== "1") {
    die("skip PHRUST_NET_TESTS is not enabled");
}
if (getenv("PHRUST_CURL_REDIRECT_POST_URL") === false || getenv("PHRUST_CURL_REDIRECT_POST_URL") === "") {
    die("skip PHRUST_CURL_REDIRECT_POST_URL is not configured");
}
?>
--FILE--
<?php
$ch = curl_init(getenv("PHRUST_CURL_REDIRECT_POST_URL"));
curl_setopt($ch, CURLOPT_RETURNTRANSFER, true);
curl_setopt($ch, CURLOPT_FOLLOWLOCATION, true);
curl_setopt($ch, CURLOPT_HEADER, true);
curl_setopt($ch, CURLOPT_HTTPHEADER, ["X-Test: yes"]);
curl_setopt($ch, CURLOPT_POSTFIELDS, ["name" => "alpha beta", "qty" => 3]);
$body = curl_exec($ch);
var_dump($body !== false);
var_dump(curl_getinfo($ch, CURLINFO_RESPONSE_CODE));
var_dump(curl_getinfo($ch, CURLINFO_HEADER_SIZE) > 0);
var_dump(str_contains($body, "OK"));
curl_close($ch);
?>
--EXPECT--
bool(true)
int(201)
bool(true)
bool(true)
