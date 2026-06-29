--TEST--
Generated objects.magic: magic __set
--DESCRIPTION--
module: objects.magic
generated timestamp: 20260627T000000Z
generator version: phpt-objects-magic-v1
reason: magic __set baseline
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
