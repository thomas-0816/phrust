--TEST--
dom: bounded DOM platform checks
--DESCRIPTION--
Focused DOM coverage for the bounded XML-backed object surface.
--EXTENSIONS--
dom
--FILE--
<?php
var_dump(extension_loaded("dom"));
var_dump(class_exists("DOMDocument", false));
var_dump(class_exists("DOMElement", false));
var_dump(class_exists("DOMAttr", false));
var_dump(class_exists("DOMText", false));
var_dump(class_exists("DOMComment", false));
var_dump(class_exists("DOMCdataSection", false));
var_dump(class_exists("DOMNodeList", false));
var_dump(class_exists("DOMNode", false));
var_dump(class_exists("DOMXPath", false));
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
bool(false)
