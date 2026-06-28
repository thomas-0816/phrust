--TEST--
dom: platform checks stay negative for policy harness
--DESCRIPTION--
Focused XML-family policy coverage for DOM platform visibility.
--FILE--
<?php
var_dump(extension_loaded("dom"));
var_dump(class_exists("DOMDocument", false));
var_dump(class_exists("DOMElement", false));
var_dump(class_exists("DOMNode", false));
var_dump(class_exists("DOMXPath", false));
?>
--EXPECT--
bool(false)
bool(false)
bool(false)
bool(false)
bool(false)
