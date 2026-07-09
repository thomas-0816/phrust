--TEST--
mbstring: bounded UTF-8 MVP platform checks
--DESCRIPTION--
Focused mbstring platform coverage. Reference output captured from
PHP 8.5.7 php-src with --enable-mbstring --disable-mbregex.
--SKIPIF--
<?php
if (!extension_loaded("mbstring")) {
    die("skip mbstring extension not loaded\n");
}
?>
--FILE--
<?php
var_dump(extension_loaded("mbstring"));
foreach ([
    "mb_strlen",
    "mb_substr",
    "mb_strtolower",
    "mb_strtoupper",
    "mb_strpos",
    "mb_stripos",
    "mb_detect_encoding",
    "mb_check_encoding",
    "mb_internal_encoding",
    "mb_convert_encoding",
] as $function) {
    echo $function, ":", function_exists($function) ? "yes\n" : "no\n";
}
?>
--EXPECT--
bool(true)
mb_strlen:yes
mb_substr:yes
mb_strtolower:yes
mb_strtoupper:yes
mb_strpos:yes
mb_stripos:yes
mb_detect_encoding:yes
mb_check_encoding:yes
mb_internal_encoding:yes
mb_convert_encoding:yes
