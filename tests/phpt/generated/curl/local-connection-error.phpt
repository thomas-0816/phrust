--TEST--
curl: local connection failure exposes libcurl error state
--DESCRIPTION--
Generated coverage for libcurl-backed error and errno propagation on a deterministic local connection failure.
--SKIPIF--
<?php
if (!extension_loaded("curl")) { die("skip curl extension is not loaded"); }
?>
--FILE--
<?php
$ch = curl_init("http://127.0.0.1:1/no-service");
curl_setopt($ch, CURLOPT_RETURNTRANSFER, true);
curl_setopt($ch, CURLOPT_CONNECTTIMEOUT_MS, 50);
curl_setopt($ch, CURLOPT_TIMEOUT_MS, 100);
var_dump(curl_exec($ch));
var_dump(curl_errno($ch) !== 0);
var_dump(curl_error($ch) !== "");
curl_close($ch);
?>
--EXPECT--
bool(false)
bool(true)
bool(true)
