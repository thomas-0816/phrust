--TEST--
Generated zend.objects: magic __set
--DESCRIPTION--
module: zend.objects
generated timestamp: 20260627T000000Z
generator version: phpt-objects-magic-v1
reason: Prompt 14.7 magic __set baseline
--FILE--
<?php
class MagicSetBox {
    public $log = "";

    public function __set($name, $value) {
        $this->log = $name . "=" . $value;
    }
}

$box = new MagicSetBox();
$box->missing = "value";
echo $box->log, "\n";
?>
--EXPECT--
missing=value
