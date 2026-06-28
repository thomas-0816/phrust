--TEST--
json: decode object and array basics
--DESCRIPTION--
Generated focused Prompt 17.1 coverage for json_decode() associative-array and stdClass object output.
--FILE--
<?php
var_dump(json_decode('{"a":1,"b":[true,null]}', true));
var_dump(json_decode('{"a":1}'));
var_dump(json_decode('[1,2,3]'));
?>
--EXPECTF--
array(2) {
  ["a"]=>
  int(1)
  ["b"]=>
  array(2) {
    [0]=>
    bool(true)
    [1]=>
    NULL
  }
}
object(stdClass)#%d (1) {
  ["a"]=>
  int(1)
}
array(3) {
  [0]=>
  int(1)
  [1]=>
  int(2)
  [2]=>
  int(3)
}
