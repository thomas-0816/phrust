--TEST--
Generated zend.objects: magic __callStatic
--DESCRIPTION--
module: zend.objects
generated timestamp: 20260627T000000Z
generator version: phpt-objects-magic-v1
reason: magic __callStatic baseline
--FILE--
<?php
class MagicCallStaticBox {
    public static function __callStatic($name, $args) {
        return $name . ":" . $args[0] . ":" . count($args);
    }
}

echo MagicCallStaticBox::missing("a", "b"), "\n";
?>
--EXPECT--
missing:a:2
