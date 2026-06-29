--TEST--
Generated objects.traits: trait method precedence
--DESCRIPTION--
module: objects.traits
generated timestamp: 20260628T000000Z
generator version: phpt-objects-advanced-v1
reason: trait method precedence baseline
--FILE--
<?php
trait TraitPrecedenceLeft {
    public function label() {
        return "left";
    }
}

trait TraitPrecedenceRight {
    public function label() {
        return "right";
    }
}

class TraitPrecedenceBox {
    use TraitPrecedenceLeft, TraitPrecedenceRight {
        TraitPrecedenceLeft::label insteadof TraitPrecedenceRight;
    }
}

$box = new TraitPrecedenceBox();
echo $box->label(), "\n";
?>
--EXPECT--
left
