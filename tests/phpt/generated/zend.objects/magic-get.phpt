--TEST--
Generated zend.objects: magic __get
--DESCRIPTION--
module: zend.objects
generated timestamp: 20260627T000000Z
generator version: phpt-objects-magic-v1
reason: Prompt 14.7 magic __get baseline
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
