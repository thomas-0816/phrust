--TEST--
simplexml: platform checks stay negative for policy harness
--DESCRIPTION--
Focused XML-family policy coverage for SimpleXML platform visibility.
--FILE--
<?php
var_dump(extension_loaded("simplexml"));
var_dump(class_exists("SimpleXMLElement", false));
var_dump(class_exists("SimpleXMLIterator", false));
var_dump(function_exists("simplexml_load_string"));
var_dump(function_exists("simplexml_load_file"));
var_dump(function_exists("simplexml_import_dom"));
?>
--EXPECT--
bool(false)
bool(false)
bool(false)
bool(false)
bool(false)
bool(false)
