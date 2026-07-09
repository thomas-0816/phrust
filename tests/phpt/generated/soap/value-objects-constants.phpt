--TEST--
soap: value objects and constants
--DESCRIPTION--
Focused SOAP coverage for constants and constructor-backed value objects.
--SKIPIF--
<?php
if (basename(PHP_BINARY) !== "phrust-php") {
    die("skip phrust-only SOAP facade fixture");
}
?>
--FILE--
<?php
echo SOAP_1_1, " ", SOAP_1_2, " ", SOAP_LITERAL, " ", SOAP_DOCUMENT, "\n";
echo XSD_STRING, " ", SOAP_ENC_ARRAY, " ", WSDL_CACHE_BOTH, "\n";
echo XSD_NAMESPACE, "\n";

$param = new SoapParam(123, "count");
var_dump($param->param_name, $param->param_data);

$header = new SoapHeader("urn:test", "Auth", "token", true, "actor");
var_dump($header->namespace, $header->name, $header->data, $header->mustUnderstand, $header->actor);

$var = new SoapVar("payload", XSD_STRING, "Payload", "urn:type", "payloadNode", "urn:node");
var_dump($var->enc_type, $var->enc_value, $var->enc_stype, $var->enc_ns, $var->enc_name, $var->enc_namens);

$fault = new SoapFault(["urn:fault", "Client"], "Broken", "actor", ["detail" => 1], "faultName", "headerFault", "en");
var_dump(is_soap_fault($fault), $fault instanceof SoapFault);
var_dump($fault->faultcodens, $fault->faultcode, $fault->faultstring, $fault->faultactor, $fault->detail["detail"], $fault->_name, $fault->headerfault, $fault->lang);
echo $fault->__toString(), "\n";
?>
--EXPECT--
1 2 2 2
101 300 3
http://www.w3.org/2001/XMLSchema
string(5) "count"
int(123)
string(8) "urn:test"
string(4) "Auth"
string(5) "token"
bool(true)
string(5) "actor"
int(101)
string(7) "payload"
string(7) "Payload"
string(8) "urn:type"
string(11) "payloadNode"
string(8) "urn:node"
bool(true)
bool(true)
string(9) "urn:fault"
string(6) "Client"
string(6) "Broken"
string(5) "actor"
int(1)
string(9) "faultName"
string(11) "headerFault"
string(2) "en"
SoapFault exception: [Client] Broken
