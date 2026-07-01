--TEST--
wp.db-network: curl WordPress HTTP option set
--SKIPIF--
<?php
if (!extension_loaded("curl")) die("skip curl extension is not loaded");
if (getenv("PHRUST_NET_TESTS") !== "1") die("skip PHRUST_NET_TESTS is not enabled");
if (getenv("PHRUST_CURL_TEST_URL") === false || getenv("PHRUST_CURL_TEST_URL") === "") die("skip PHRUST_CURL_TEST_URL is not configured");
?>
--FILE--
<?php
$ch = curl_init(getenv("PHRUST_CURL_TEST_URL"));
var_dump(curl_setopt_array($ch, array(
    CURLOPT_RETURNTRANSFER => true,
    CURLOPT_USERAGENT => "WordPress/6.x; phrust",
    CURLOPT_REFERER => getenv("PHRUST_CURL_TEST_URL"),
    CURLOPT_ENCODING => "",
    CURLOPT_HTTP_VERSION => 0,
    CURLOPT_CONNECTTIMEOUT_MS => 500,
    CURLOPT_MAXREDIRS => 2,
    CURLOPT_FAILONERROR => false,
    CURLOPT_HTTPHEADER => array("X-Phrust-Test: 1"),
)));
$body = curl_exec($ch);
var_dump($body !== false);
var_dump(curl_errno($ch));
curl_close($ch);
?>
--EXPECT--
bool(true)
bool(true)
int(0)
