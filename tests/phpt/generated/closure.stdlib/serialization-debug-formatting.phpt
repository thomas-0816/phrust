--TEST--
closure.stdlib: serialization and debug output formatting
--DESCRIPTION--
Generated closure stdlib coverage for serialize, unserialize, var_export,
var_dump, print_r, and explicit unsupported reference-record parsing.
--FILE--
<?php
class ClosureStdlibBox {
    public $name = "box";
    public $items = [1, 2];
}

$value = ["a" => 1, "b" => true, "c" => null];
$encoded = serialize($value);
echo $encoded, "\n";
var_dump(unserialize($encoded));
echo var_export($value, true), "\n";
print_r($value);
$box = new ClosureStdlibBox();
var_dump($box->name, $box->items);
error_reporting(0);
var_dump(@unserialize("R:1;"));
?>
--EXPECTF--
a:3:{s:1:"a";i:1;s:1:"b";b:1;s:1:"c";N;}
array(3) {
  ["a"]=>
  int(1)
  ["b"]=>
  bool(true)
  ["c"]=>
  NULL
}
array (
  'a' => 1,
  'b' => true,
  'c' => NULL,
)
Array
(
    [a] => 1
    [b] => 1
    [c] =>%w
)
string(3) "box"
array(2) {
  [0]=>
  int(1)
  [1]=>
  int(2)
}
bool(false)
