--TEST--
wp.db-network: curl local HTTP smoke through explicit URL gate
--DESCRIPTION--
contract for real HTTP execution. The URL must point at a local
test server; public internet endpoints are not part of this module.
--SKIPIF--
<?php
if (!extension_loaded("curl")) {
    die("skip curl extension is not loaded");
}
if (getenv("PHRUST_NET_TESTS") !== "1") {
    die("skip PHRUST_NET_TESTS is not enabled");
}
if (getenv("PHRUST_CURL_TEST_URL") === false || getenv("PHRUST_CURL_TEST_URL") === "") {
    die("skip PHRUST_CURL_TEST_URL is not configured");
}
?>
--FILE--
<?php
$ch = curl_init(getenv("PHRUST_CURL_TEST_URL"));
curl_setopt($ch, CURLOPT_RETURNTRANSFER, true);
curl_setopt($ch, CURLOPT_TIMEOUT, 2);
$body = curl_exec($ch);
var_dump($body !== false);
var_dump(curl_getinfo($ch, CURLINFO_RESPONSE_CODE) > 0);
curl_close($ch);
?>
--EXPECT--
bool(true)
bool(true)
