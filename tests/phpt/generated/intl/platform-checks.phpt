--TEST--
intl: platform checks stay negative for disabled strategy
--DESCRIPTION--
Focused intl stub coverage for Composer-style platform checks.
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
?>
--EXPECT--
bool(false)
bool(false)
bool(false)
bool(false)
bool(false)
bool(false)
bool(false)
bool(false)
