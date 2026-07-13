--TEST--
opcache: request-local API facade
--DESCRIPTION--
Generated OPcache coverage for PHP-visible facade functions, deterministic
request-local compile status, invalidation, and configuration/status arrays.
--SKIPIF--
<?php
if (basename(PHP_BINARY) !== "phrust-php") {
    die("skip phrust-only OPcache facade fixture");
}
?>
--FILE--
<?php
var_dump(extension_loaded("opcache"));
$file = __DIR__ . "/opcache-facade-target.php";
$bad = __DIR__ . "/opcache-facade-bad.php";
file_put_contents($file, "<?php return 42;\n");
file_put_contents($bad, "<?php function {\n");
var_dump(function_exists("opcache_get_status"));
var_dump(opcache_is_script_cached($file));
var_dump(opcache_compile_file($file));
var_dump(opcache_is_script_cached($file));
error_reporting(0);
var_dump(opcache_compile_file($bad));
var_dump(opcache_is_script_cached($bad));
error_reporting(E_ALL);
$status = opcache_get_status();
var_dump(is_array($status));
var_dump($status["opcache_enabled"]);
var_dump($status["opcache_statistics"]["num_cached_scripts"] >= 1);
var_dump(isset($status["scripts"][$file]));
$config = opcache_get_configuration();
var_dump(is_array($config["directives"]));
var_dump($config["version"]["opcache_product_name"] !== "");
var_dump(opcache_invalidate($file));
var_dump(opcache_is_script_cached($file));
var_dump(opcache_reset());
@unlink($file);
@unlink($bad);
?>
--EXPECT--
bool(true)
bool(true)
bool(false)
bool(true)
bool(true)
bool(false)
bool(false)
bool(true)
bool(true)
bool(true)
bool(true)
bool(true)
bool(true)
bool(true)
bool(false)
bool(true)
