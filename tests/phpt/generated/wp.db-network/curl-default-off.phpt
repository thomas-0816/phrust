--TEST--
wp.db-network: curl HTTP execution is default-off
--DESCRIPTION--
Prompt 3.6 contract: cURL network execution requires the explicit
PHRUST_NET_TESTS=1 gate and never fakes success.
--SKIPIF--
<?php
if (!extension_loaded("curl")) {
    die("skip curl extension is not loaded");
}
if (getenv("PHRUST_NET_TESTS") === "1") {
    die("skip PHRUST_NET_TESTS is enabled");
}
?>
--FILE--
<?php
$ch = curl_init("http://127.0.0.1:1/wp-json");
curl_setopt($ch, CURLOPT_RETURNTRANSFER, true);
var_dump(curl_exec($ch));
var_dump(curl_errno($ch) > 0);
var_dump(curl_error($ch) !== "");
curl_close($ch);
?>
--EXPECT--
bool(false)
bool(true)
bool(true)
