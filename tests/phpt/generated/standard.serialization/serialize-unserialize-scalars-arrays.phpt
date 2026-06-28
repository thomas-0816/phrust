--TEST--
standard.serialization: serialize and unserialize scalar and array values
--FILE--
<?php
$values = [NULL, true, false, 7, -3, 1.5, "abc", [1 => "x", "k" => false, 2 => [NULL]]];
foreach ($values as $value) {
    $serialized = serialize($value);
    echo $serialized, "\n";
    var_dump(unserialize($serialized));
}
?>
--EXPECT--
N;
NULL
b:1;
bool(true)
b:0;
bool(false)
i:7;
int(7)
i:-3;
int(-3)
d:1.5;
float(1.5)
s:3:"abc";
string(3) "abc"
a:3:{i:1;s:1:"x";s:1:"k";b:0;i:2;a:1:{i:0;N;}}
array(3) {
  [1]=>
  string(1) "x"
  ["k"]=>
  bool(false)
  [2]=>
  array(1) {
    [0]=>
    NULL
  }
}
