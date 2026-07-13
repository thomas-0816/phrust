--TEST--
ftp opt-in live endpoint smoke
--SKIPIF--
<?php
if (!extension_loaded("ftp")) die("skip ftp extension not loaded");
if (getenv("PHRUST_NET_TESTS") !== "1") die("skip PHRUST_NET_TESTS not enabled");
if (!getenv("PHRUST_FTP_LIVE_ENDPOINT")) die("skip PHRUST_FTP_LIVE_ENDPOINT not set");
if (!getenv("PHRUST_FTP_LIVE_USER")) die("skip PHRUST_FTP_LIVE_USER not set");
if (!getenv("PHRUST_FTP_LIVE_PASSWORD")) die("skip PHRUST_FTP_LIVE_PASSWORD not set");
?>
--FILE--
<?php
$endpoint = getenv("PHRUST_FTP_LIVE_ENDPOINT");
$parts = explode(":", $endpoint, 2);
$host = $parts[0];
$port = isset($parts[1]) ? (int) $parts[1] : 21;
$path = getenv("PHRUST_FTP_LIVE_PATH") ?: ".";
$ftp = ftp_connect($host, $port, 3);
var_dump($ftp instanceof FTP\Connection);
var_dump(ftp_login($ftp, getenv("PHRUST_FTP_LIVE_USER"), getenv("PHRUST_FTP_LIVE_PASSWORD")));
var_dump(is_string(ftp_pwd($ftp)));
var_dump(ftp_pasv($ftp, true));
$list = ftp_nlist($ftp, $path);
var_dump(is_array($list) || $list === false);
var_dump(ftp_close($ftp));
?>
--EXPECT--
bool(true)
bool(true)
bool(true)
bool(true)
bool(true)
bool(true)
