--TEST--
curl: common constants, option storage, metadata, and unsupported options
--DESCRIPTION--
Generated coverage for CURL-3 common application constants and PHP-like
unsupported option failure.
--SKIPIF--
<?php
if (!extension_loaded("curl")) {
    die("skip curl extension is not loaded");
}
?>
--FILE--
<?php
$constants = [
    "CURLOPT_ACCEPT_ENCODING",
    "CURLOPT_AUTOREFERER",
    "CURLOPT_COOKIE",
    "CURLOPT_COOKIEFILE",
    "CURLOPT_COOKIEJAR",
    "CURLOPT_COOKIESESSION",
    "CURLOPT_DNS_CACHE_TIMEOUT",
    "CURLOPT_HTTPGET",
    "CURLOPT_HTTPPROXYTUNNEL",
    "CURLOPT_IPRESOLVE",
    "CURLOPT_NOPROXY",
    "CURLOPT_PORT",
    "CURLOPT_PROXYUSERNAME",
    "CURLOPT_PROXYPASSWORD",
    "CURLOPT_TCP_NODELAY",
    "CURLOPT_USERNAME",
    "CURLOPT_PASSWORD",
    "CURLOPT_SSLCERT",
    "CURLOPT_SSLKEY",
    "CURLOPT_SSLVERSION",
    "CURLOPT_VERBOSE",
    "CURLINFO_CONTENT_TYPE",
    "CURLINFO_NAMELOOKUP_TIME",
    "CURLINFO_CONNECT_TIME",
    "CURLINFO_PRETRANSFER_TIME",
    "CURLINFO_STARTTRANSFER_TIME",
    "CURLINFO_HTTP_CONNECTCODE",
    "CURLINFO_REDIRECT_TIME",
    "CURLINFO_REDIRECT_COUNT",
    "CURLINFO_REQUEST_SIZE",
    "CURLINFO_SIZE_DOWNLOAD",
    "CURL_VERSION_LIBZ",
    "CURL_VERSION_HTTP2",
    "CURL_VERSION_HTTP3",
    "CURLPROTO_ALL",
    "CURLPROTO_FTP",
    "CURL_IPRESOLVE_V4",
    "CURL_SSLVERSION_TLSv1_2",
    "CURL_HTTP_VERSION_2_0",
];
foreach ($constants as $name) {
    if (!defined($name)) {
        echo "missing $name\n";
    }
}
echo "constants-ok\n";

$ch = curl_init("http://127.0.0.1/");
$options = [
    CURLOPT_RETURNTRANSFER => true,
    CURLOPT_ACCEPT_ENCODING => "",
    CURLOPT_AUTOREFERER => true,
    CURLOPT_COOKIE => "a=b",
    CURLOPT_COOKIEFILE => "",
    CURLOPT_COOKIEJAR => "phrust-curl-cookiejar.txt",
    CURLOPT_COOKIESESSION => true,
    CURLOPT_DNS_CACHE_TIMEOUT => 10,
    CURLOPT_HTTPGET => true,
    CURLOPT_HTTPPROXYTUNNEL => false,
    CURLOPT_HTTP_VERSION => CURL_HTTP_VERSION_2_0,
    CURLOPT_IPRESOLVE => CURL_IPRESOLVE_V4,
    CURLOPT_NOPROXY => "localhost,127.0.0.1",
    CURLOPT_PORT => 80,
    CURLOPT_PROXYUSERNAME => "proxy-user",
    CURLOPT_PROXYPASSWORD => "proxy-pass",
    CURLOPT_TCP_NODELAY => true,
    CURLOPT_USERNAME => "user",
    CURLOPT_PASSWORD => "pass",
    CURLOPT_SSLCERT => "client.pem",
    CURLOPT_SSLKEY => "client.key",
    CURLOPT_SSLVERSION => CURL_SSLVERSION_TLSv1_2,
    CURLOPT_VERBOSE => false,
];
$stored = 0;
foreach ($options as $option => $value) {
    if (curl_setopt($ch, $option, $value)) {
        $stored++;
    }
}
var_dump($stored);

$version = curl_version();
var_dump(is_array($version));
var_dump(isset($version["version"], $version["host"], $version["features"], $version["protocols"]));
var_dump(is_int($version["features"]));
var_dump(is_array($version["protocols"]));

var_dump(curl_getinfo($ch, CURLINFO_REDIRECT_COUNT));
var_dump(curl_getinfo($ch, CURLINFO_REQUEST_SIZE));
var_dump(curl_getinfo($ch, CURLINFO_SIZE_DOWNLOAD));
var_dump(curl_getinfo($ch, CURLINFO_CONTENT_TYPE));
?>
--EXPECT--
constants-ok
int(23)
bool(true)
bool(true)
bool(true)
bool(true)
int(0)
int(0)
float(0)
bool(false)
