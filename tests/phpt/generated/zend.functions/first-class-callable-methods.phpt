--TEST--
Generated zend.functions: first-class callables from methods and statics
--DESCRIPTION--
module: zend.functions
generated timestamp: 20260627T000000Z
generator version: phpt-zend-functions-v1
reason: first-class callable syntax resolves instance methods ($obj->m(...)), static methods (Cls::m(...)) and functions (f(...)) to invocable callables (Zend/tests/first_class_callable/first_class_callable_003.phpt)
--FILE--
<?php
class Greeter {
    public function hi($name) { return "hi $name"; }
    public static function yo($name) { return "yo $name"; }
}
$g = new Greeter();
$instance = $g->hi(...);
$static = Greeter::yo(...);
$fn = strtoupper(...);
echo $instance("a"), "\n";
echo $static("b"), "\n";
echo $fn("c"), "\n";
?>
--EXPECT--
hi a
yo b
C
