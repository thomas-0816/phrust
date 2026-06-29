--TEST--
Generated objects.magic: magic __unset
--DESCRIPTION--
module: objects.magic
generated timestamp: 20260627T000000Z
generator version: phpt-objects-magic-v1
reason: magic __unset baseline
--FILE--
<?php
class MagicUnsetBox {
    public $log = "";

    public function __unset($name) {
        $this->log = "unset:" . $name;
    }
}

$box = new MagicUnsetBox();
unset($box->missing);
echo $box->log, "\n";
?>
--EXPECT--
unset:missing
