--TEST--
Generated objects.magic: magic __get
--DESCRIPTION--
module: objects.magic
generated timestamp: 20260627T000000Z
generator version: phpt-objects-magic-v1
reason: magic __get baseline
--FILE--
<?php
class MagicGetBox {
    public function __get($name) {
        return "get:" . $name;
    }
}

$box = new MagicGetBox();
echo $box->missing, "\n";
?>
--EXPECT--
get:missing
