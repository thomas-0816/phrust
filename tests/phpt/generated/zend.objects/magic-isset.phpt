--TEST--
Generated zend.objects: magic __isset
--DESCRIPTION--
module: zend.objects
generated timestamp: 20260627T000000Z
generator version: phpt-objects-magic-v1
reason: magic __isset baseline
--FILE--
<?php
class MagicIssetBox {
    public function __isset($name) {
        echo "isset:" . $name . "\n";
        if ($name == "present") {
            return true;
        }
        return false;
    }
}

$box = new MagicIssetBox();
var_dump(isset($box->present));
var_dump(isset($box->missing));
?>
--EXPECT--
isset:present
bool(true)
isset:missing
bool(false)
