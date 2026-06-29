--TEST--
xml: parser error code and string MVP
--DESCRIPTION--
Generated XML parser coverage for deterministic parse error state helpers.
--EXTENSIONS--
xml
--FILE--
<?php
$parser = xml_parser_create();
var_dump(function_exists("xml_get_error_code"));
var_dump(function_exists("xml_error_string"));
var_dump(xml_parse($parser, '<root><child></root>', true));
$code = xml_get_error_code($parser);
var_dump($code > 0);
var_dump(xml_error_string($code));
?>
--EXPECT--
bool(true)
bool(true)
int(0)
bool(true)
string(14) "Mismatched tag"
