--TEST--
closure.stdlib: SPL and Reflection selected introspection
--DESCRIPTION--
Generated closure stdlib coverage for selected SPL iterator/container helpers
and Reflection function/class metadata used by stdlib parity gates.
--FILE--
<?php
$it = new ArrayIterator(["a" => 1, "b" => 2]);
var_dump(iterator_to_array($it));
var_dump(iterator_count(new ArrayIterator([1, 2, 3])));
var_dump(class_exists("ArrayIterator"));
var_dump(interface_exists("Iterator"));

function closure_stdlib_reflect($first, ...$rest) {
    return $first;
}
$rf = new ReflectionFunction("closure_stdlib_reflect");
var_dump($rf->getName());
var_dump($rf->getNumberOfParameters());
var_dump($rf->getParameters()[1]->isVariadic());
$rc = new ReflectionClass("ArrayIterator");
var_dump($rc->getName());
var_dump($rc->isInternal());
?>
--EXPECT--
array(2) {
  ["a"]=>
  int(1)
  ["b"]=>
  int(2)
}
int(3)
bool(true)
bool(true)
string(22) "closure_stdlib_reflect"
int(2)
bool(true)
string(13) "ArrayIterator"
bool(true)
