--TEST--
xsl: platform checks stay negative for policy harness
--DESCRIPTION--
Focused XML-family policy coverage for XSL platform visibility.
--FILE--
<?php
var_dump(extension_loaded("xsl"));
var_dump(class_exists("XSLTProcessor", false));
var_dump(defined("XSL_CLONE_AUTO"));
?>
--EXPECT--
bool(false)
bool(false)
bool(false)
