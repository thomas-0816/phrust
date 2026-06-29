--TEST--
Generated objects.magic: magic __toString
--DESCRIPTION--
module: objects.magic
generated timestamp: 20260627T000000Z
generator version: phpt-objects-magic-v1
reason: magic __toString baseline
--FILE--
<?php
class MagicStringBox {
    public function __toString() {
        return "string-value";
    }
}

echo "value=" . new MagicStringBox(), "\n";
?>
--EXPECT--
value=string-value
