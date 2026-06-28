--TEST--
soap: platform checks stay negative for policy harness
--DESCRIPTION--
Focused XML-family policy coverage for SOAP platform visibility.
--FILE--
<?php
var_dump(extension_loaded("soap"));
var_dump(class_exists("SoapClient", false));
var_dump(class_exists("SoapServer", false));
var_dump(class_exists("SoapFault", false));
var_dump(class_exists("SoapHeader", false));
?>
--EXPECT--
bool(false)
bool(false)
bool(false)
bool(false)
bool(false)
