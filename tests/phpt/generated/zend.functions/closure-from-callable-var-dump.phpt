--TEST--
Generated zend.functions: Closure::fromCallable exposes Closure dump shape
--DESCRIPTION--
module: zend.functions
generated timestamp: 20260627T000000Z
generator version: phpt-zend-functions-v1
reason: Closure::fromCallable returns a Closure instance with basic var_dump object shape (Zend/tests/closures/closure_061.phpt)
--FILE--
<?php
function prompt13_closure_source($value) {
    return strtoupper($value);
}

$closure = Closure::fromCallable("prompt13_closure_source");
echo ($closure instanceof Closure) ? "instance\n" : "not-instance\n";
echo $closure("ok"), "\n";
var_dump($closure);
?>
--EXPECTF--
instance
OK
object(Closure)#%d (%d) {
%A}
