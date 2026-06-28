--TEST--
standard.variables: debug formatting smoke
--DESCRIPTION--
Generated focused coverage for basic type helpers and debug formatting.
--FILE--
<?php
$values = [
    "null" => null,
    "bool" => true,
    "int" => 42,
    "float" => 2.5,
    "string" => "php",
    "array" => ["a" => 1, "b" => false],
];

foreach ($values as $name => $value) {
    echo $name, ":", gettype($value), ":";
    var_dump(is_scalar($value));
}

var_dump($values["array"]);
print_r($values["array"]);
var_export($values["array"]);
echo "\n";
?>
--EXPECTF--
null:NULL:bool(false)
bool:boolean:bool(true)
int:integer:bool(true)
float:double:bool(true)
string:string:bool(true)
array:array:bool(false)
array(2) {
  ["a"]=>
  int(1)
  ["b"]=>
  bool(false)
}
Array
(
    [a] => 1
    [b] =>%s
)
array (
  'a' => 1,
  'b' => false,
)
