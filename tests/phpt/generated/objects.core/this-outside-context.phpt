--TEST--
Generated objects.core: $this outside method context is Error
--DESCRIPTION--
module: objects.core
generated timestamp: 20260628T000000Z
generator version: phpt-objects-core-v1
reason: Branch 1 object-core invalid $this outside method coverage
original path: Zend/tests/assign_to_obj_002.phpt
--FILE--
<?php
try {
    $this->a = new stdClass;
} catch (Error $e) {
    echo $e->getMessage(), "\n";
}
?>
--EXPECT--
Using $this when not in object context
