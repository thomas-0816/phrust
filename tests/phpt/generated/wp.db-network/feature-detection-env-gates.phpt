--TEST--
wp.db-network: feature detection is stable across capability env gates
--DESCRIPTION--
coverage for WordPress extension/function/class probes before and after the
local DB/network capability environment variables are set. This fixture must
not open MySQL or cURL sockets.
--SKIPIF--
<?php
if (!extension_loaded("mysqli")) {
    die("skip mysqli extension is not loaded");
}
if (!extension_loaded("curl")) {
    die("skip curl extension is not loaded");
}
if (!extension_loaded("openssl")) {
    die("skip openssl extension is not loaded");
}
?>
--FILE--
<?php
function probe($label) {
    echo $label, "\n";
    var_dump(extension_loaded("mysqli"));
    var_dump(function_exists("mysqli_connect"));
    var_dump(function_exists("mysqli_prepare"));
    var_dump(class_exists("mysqli_stmt", false));
    var_dump(extension_loaded("curl"));
    var_dump(function_exists("curl_exec"));
    var_dump(function_exists("curl_setopt_array"));
    var_dump(class_exists("CurlHandle", false));
    var_dump(extension_loaded("openssl"));
    var_dump(function_exists("openssl_verify"));
    var_dump(function_exists("openssl_error_string"));
}

probe("before");
putenv("PHRUST_MYSQL_TEST_DSN=mysql://user:pass@127.0.0.1:3306/wordpress_test");
putenv("PHRUST_NET_TESTS=1");
putenv("PHRUST_CURL_TEST_URL=http://127.0.0.1:1/");
probe("after");
?>
--EXPECT--
before
bool(true)
bool(true)
bool(true)
bool(true)
bool(true)
bool(true)
bool(true)
bool(true)
bool(true)
bool(true)
bool(true)
after
bool(true)
bool(true)
bool(true)
bool(true)
bool(true)
bool(true)
bool(true)
bool(true)
bool(true)
bool(true)
bool(true)
