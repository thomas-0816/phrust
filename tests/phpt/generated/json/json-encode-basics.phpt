--TEST--
json: encode scalar, array, and object basics
--DESCRIPTION--
Generated focused coverage for json_encode() scalar, list, map, nested, and simple object output.
--FILE--
<?php
var_dump(json_encode(null));
var_dump(json_encode(true));
var_dump(json_encode(12));
var_dump(json_encode("x"));
var_dump(json_encode(array(1, 2, 3)));
var_dump(json_encode(array("a" => 1, "b" => array(true, null, "x"))));
var_dump(json_encode((object) array("a" => 1)));
?>
--EXPECT--
string(4) "null"
string(4) "true"
string(2) "12"
string(3) ""x""
string(7) "[1,2,3]"
string(27) "{"a":1,"b":[true,null,"x"]}"
string(7) "{"a":1}"
