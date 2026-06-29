--TEST--
intl: bounded platform checks
--DESCRIPTION--
Focused intl coverage for the bounded Unicode helper MVP.
--EXTENSIONS--
intl
--FILE--
<?php
var_dump(extension_loaded("intl"));
var_dump(function_exists("intl_get_error_code"));
var_dump(function_exists("grapheme_strlen"));
var_dump(function_exists("normalizer_normalize"));
var_dump(class_exists("Locale"));
var_dump(class_exists("NumberFormatter"));
var_dump(class_exists("Collator"));
var_dump(class_exists("IntlChar"));
var_dump(class_exists("Normalizer"));
?>
--EXPECT--
bool(true)
bool(true)
bool(true)
bool(true)
bool(true)
bool(true)
bool(true)
bool(true)
bool(true)
