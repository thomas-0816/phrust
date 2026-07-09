--TEST--
soap: bounded platform facade
--DESCRIPTION--
Focused SOAP facade coverage for platform visibility and global helpers.
--SKIPIF--
<?php
if (basename(PHP_BINARY) !== "phrust-php") {
    die("skip phrust-only SOAP facade fixture");
}
?>
--FILE--
<?php
var_dump(extension_loaded("soap"));
var_dump(function_exists("is_soap_fault"));
var_dump(function_exists("use_soap_error_handler"));
var_dump(class_exists("SoapClient", false));
var_dump(class_exists("SoapServer", false));
var_dump(class_exists("SoapFault", false));
var_dump(class_exists("SoapHeader", false));
var_dump(class_exists("SoapParam", false));
var_dump(class_exists("SoapVar", false));
var_dump(class_exists("Soap\\SoapClient", false));
var_dump(class_exists("Soap\\SoapFault", false));
var_dump(is_soap_fault(new stdClass()));
var_dump(use_soap_error_handler(false));
var_dump(use_soap_error_handler(true));
var_dump(use_soap_error_handler(false));
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
bool(true)
bool(true)
bool(false)
bool(false)
bool(false)
bool(true)
