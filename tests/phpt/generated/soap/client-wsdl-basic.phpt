--TEST--
soap: local WSDL metadata and bounded server handle
--DESCRIPTION--
Focused SOAP coverage for local WSDL parsing and XML-backed server request handling.
--SKIPIF--
<?php
if (basename(PHP_BINARY) !== "phrust-php") {
    die("skip phrust-only SOAP client fixture");
}
?>
--FILE--
<?php
$wsdl = sys_get_temp_dir() . "/phrust-soap-client-basic.wsdl";
file_put_contents($wsdl, '<definitions targetNamespace="urn:test" xmlns="http://schemas.xmlsoap.org/wsdl/" xmlns:soap="http://schemas.xmlsoap.org/wsdl/soap/"><portType name="DemoPort"><operation name="echo"/></portType><binding name="DemoBinding" type="DemoPort"><operation name="echo"><soap:operation soapAction="urn:test#echo"/></operation></binding><service name="Demo"><port name="DemoPort" binding="DemoBinding"><soap:address location="http://127.0.0.1:18081/soap"/></port></service></definitions>');

$client = new SoapClient($wsdl);
$functions = $client->__getFunctions();
echo $functions[0], "\n";
var_dump($client->__setLocation("http://127.0.0.1:18082/soap"));

$server = new SoapServer(null);
$request = '<?xml version="1.0"?><SOAP-ENV:Envelope xmlns:SOAP-ENV="http://schemas.xmlsoap.org/soap/envelope/"><SOAP-ENV:Body><ns1:echo xmlns:ns1="urn:test"><param0>hello</param0></ns1:echo></SOAP-ENV:Body></SOAP-ENV:Envelope>';
$response = $server->handle($request);
var_dump(strpos($response, "SOAP-ENV:Fault") !== false);
var_dump(strpos($server->__getLastResponse(), "callback dispatch is not implemented") !== false);
unlink($wsdl);
?>
--EXPECT--
echo
string(27) "http://127.0.0.1:18081/soap"
bool(true)
bool(true)
