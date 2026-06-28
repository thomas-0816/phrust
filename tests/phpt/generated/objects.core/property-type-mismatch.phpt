--TEST--
Generated objects.core: property type mismatch is TypeError
--DESCRIPTION--
module: objects.core
generated timestamp: 20260627T000000Z
generator version: phpt-objects-typed-properties-v1
reason: Prompt 14.6 property type mismatch baseline
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
