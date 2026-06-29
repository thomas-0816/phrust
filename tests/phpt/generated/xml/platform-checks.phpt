--TEST--
xml: bounded parser platform checks
--DESCRIPTION--
Focused XML coverage for the bounded parser surface.
--EXTENSIONS--
xml
--FILE--
<?php
var_dump(extension_loaded("xml"));
var_dump(class_exists("XMLParser", false));
var_dump(function_exists("xml_parser_create"));
var_dump(function_exists("xml_parse"));
?>
--EXPECT--
bool(true)
bool(true)
bool(true)
bool(true)
