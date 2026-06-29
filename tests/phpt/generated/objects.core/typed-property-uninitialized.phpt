--TEST--
Generated objects.core: typed property uninitialized read is Error
--DESCRIPTION--
module: objects.core
generated timestamp: 20260627T000000Z
generator version: phpt-objects-typed-properties-v1
reason: typed property uninitialized baseline
--FILE--
<?php
class Box {
    public int $value;
}

try {
    echo (new Box())->value;
} catch (Error $e) {
    echo "caught:", get_class($e), "\n";
}
?>
--EXPECT--
caught:Error
