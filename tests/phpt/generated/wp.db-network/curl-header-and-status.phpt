--TEST--
wp.db-network: curl header and status handling
--SKIPIF--
<?php
if (!extension_loaded("curl")) die("skip curl extension is not loaded");
if (getenv("PHRUST_NET_TESTS") !== "1") die("skip PHRUST_NET_TESTS is not enabled");
if (getenv("PHRUST_CURL_TEST_URL") === false || getenv("PHRUST_CURL_TEST_URL") === "") die("skip PHRUST_CURL_TEST_URL is not configured");
?>
--FILE--
<?php
$ch = curl_init(getenv("PHRUST_CURL_TEST_URL"));
curl_setopt($ch, CURLOPT_RETURNTRANSFER, true);
curl_setopt($ch, CURLOPT_HEADER, true);
$response = curl_exec($ch);
var_dump($response !== false);
var_dump(curl_getinfo($ch, CURLINFO_RESPONSE_CODE) > 0);
var_dump(curl_getinfo($ch, CURLINFO_HEADER_SIZE) > 0);
curl_reset($ch);
curl_setopt($ch, CURLOPT_URL, getenv("PHRUST_CURL_TEST_URL"));
curl_setopt($ch, CURLOPT_NOBODY, true);
curl_setopt($ch, CURLOPT_RETURNTRANSFER, true);
var_dump(curl_exec($ch) !== false);
curl_close($ch);
?>
--EXPECT--
bool(true)
bool(true)
bool(true)
bool(true)
