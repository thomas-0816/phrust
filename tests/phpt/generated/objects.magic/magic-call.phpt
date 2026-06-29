--TEST--
Generated objects.magic: magic __call
--DESCRIPTION--
module: objects.magic
generated timestamp: 20260627T000000Z
generator version: phpt-objects-magic-v1
reason: magic __call baseline
--FILE--
<?php
class MagicCallBox {
    public function __call($name, $args) {
        return $name . ":" . $args[0] . ":" . count($args);
    }
}

$box = new MagicCallBox();
echo $box->missing("a", "b"), "\n";
?>
--EXPECT--
missing:a:2
