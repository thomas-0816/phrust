--TEST--
Generated zend.objects: trait method composition
--DESCRIPTION--
module: zend.objects
generated timestamp: 20260627T000000Z
generator version: phpt-objects-traits-enums-v1
reason: trait method composition baseline
--FILE--
<?php
trait TraitMethodBoxTrait {
    public function label($value) {
        return "trait:" . $value;
    }
}

class TraitMethodBox {
    use TraitMethodBoxTrait;
}

$box = new TraitMethodBox();
echo $box->label("ok"), "\n";
?>
--EXPECT--
trait:ok
