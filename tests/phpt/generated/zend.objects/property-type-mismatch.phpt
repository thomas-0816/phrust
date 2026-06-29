--TEST--
Generated zend.objects: property type mismatch is TypeError
--DESCRIPTION--
module: zend.objects
generated timestamp: 20260627T000000Z
generator version: phpt-objects-typed-properties-v1
reason: property type mismatch baseline
--FILE--
<?php
class Box {
    public int $value;
}

$box = new Box();
try {
    $box->value = [];
} catch (TypeError $e) {
    echo "caught:", get_class($e), "\n";
}
?>
--EXPECT--
caught:TypeError
