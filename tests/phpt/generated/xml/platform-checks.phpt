--TEST--
xml: platform checks stay negative for policy harness
--DESCRIPTION--
Focused XML-family policy coverage for XML parser platform visibility.
--FILE--
<?php
var_dump(extension_loaded("xml"));
var_dump(class_exists("XMLParser", false));
var_dump(function_exists("xml_parser_create"));
var_dump(function_exists("xml_parse"));
var_dump(function_exists("xml_error_string"));
var_dump(defined("XML_ERROR_NONE"));
?>
--EXPECT--
bool(false)
bool(false)
bool(false)
bool(false)
bool(false)
bool(false)
