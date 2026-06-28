--TEST--
Generated objects.core: new C allocates an object
--DESCRIPTION--
module: objects.core
generated timestamp: 20260628T000000Z
generator version: phpt-objects-core-v1
reason: Branch 1 object-core plain construction baseline
--FILE--
<?php
class C {
}

$c = new C();
echo get_class($c), "\n";
?>
--EXPECT--
C
