--TEST--
Generated zend.objects: magic __invoke
--DESCRIPTION--
module: zend.objects
generated timestamp: 20260627T000000Z
generator version: phpt-objects-magic-v1
reason: magic __invoke baseline
--FILE--
<?php
class MagicInvokeBox {
    public function __invoke($left, $right) {
        return $left . ":" . $right;
    }
}

$box = new MagicInvokeBox();
echo $box("left", "right"), "\n";
?>
--EXPECT--
left:right
