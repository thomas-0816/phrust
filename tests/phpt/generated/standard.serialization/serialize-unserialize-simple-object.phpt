--TEST--
standard.serialization: serialize and unserialize simple public objects
--FILE--
<?php
class Box {
    public $value = 7;
    public $name = "box";
}
$box = new Box();
$serialized = serialize($box);
echo $serialized, "\n";
var_dump(unserialize($serialized));
?>
--EXPECTF--
O:3:"Box":2:{s:5:"value";i:7;s:4:"name";s:3:"box";}
object(Box)#%d (2) {
  ["value"]=>
  int(7)
  ["name"]=>
  string(3) "box"
}
