--TEST--
Generated zend.objects: enum method
--DESCRIPTION--
module: zend.objects
generated timestamp: 20260627T000000Z
generator version: phpt-objects-traits-enums-v1
reason: Prompt 14.9 enum method baseline
--FILE--
<?php
enum ObjectMethodDirection {
    case Up;

    public function label() {
        return "up";
    }
}

echo ObjectMethodDirection::Up->label(), "\n";
?>
--EXPECT--
up
