--TEST--
simplexml: bounded SimpleXML platform checks
--DESCRIPTION--
Focused SimpleXML coverage for the bounded XML-backed object surface.
--EXTENSIONS--
simplexml
--FILE--
<?php
var_dump(extension_loaded("simplexml"));
var_dump(class_exists("SimpleXMLElement", false));
var_dump(class_exists("SimpleXMLIterator", false));
var_dump(function_exists("simplexml_load_string"));
var_dump(function_exists("simplexml_load_file"));
var_dump(function_exists("simplexml_import_dom"));
var_dump(function_exists("dom_import_simplexml"));
?>
--EXPECT--
bool(true)
bool(true)
bool(false)
bool(true)
bool(true)
bool(true)
bool(true)
