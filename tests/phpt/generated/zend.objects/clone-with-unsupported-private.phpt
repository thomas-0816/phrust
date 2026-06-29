--TEST--
Generated zend.objects: clone-with unsupported private property gap
--DESCRIPTION--
module: zend.objects
generated timestamp: 20260627T000000Z
generator version: phpt-objects-clone-v1
reason: clone-with unsupported private property gap
--FILE--
<?php
class CloneWithPrivateBox {
    private $secret = "old";
}

$original = new CloneWithPrivateBox();
try {
    $copy = clone($original, ["secret" => "new"]);
} catch (Error $e) {
    echo get_class($e), "\n";
}
?>
--EXPECT--
Error
