--TEST--
Generated zend.objects: trait method alias
--DESCRIPTION--
module: zend.objects
generated timestamp: 20260627T000000Z
generator version: phpt-objects-traits-enums-v1
reason: trait method alias baseline
--FILE--
<?php
trait TraitAliasBoxTrait {
    public function base() {
        return "base";
    }
}

class TraitAliasBox {
    use TraitAliasBoxTrait {
        base as alias;
    }
}

$box = new TraitAliasBox();
echo $box->base(), "|", $box->alias(), "\n";
?>
--EXPECT--
base|base
