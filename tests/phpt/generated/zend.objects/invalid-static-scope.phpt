--TEST--
Generated zend.objects: invalid static access is catchable Error
--DESCRIPTION--
module: zend.objects
generated timestamp: 20260627T000000Z
generator version: phpt-objects-static-v1
reason: invalid static access catchable Error
--FILE--
<?php
class ScopeTarget {
    public function name() {
        return "ScopeTarget";
    }
}

try {
    ScopeTarget::$missing;
} catch (Error $e) {
    echo "read\n";
}

try {
    ScopeTarget::$missing = 2;
} catch (Error $e) {
    echo "write\n";
}

try {
    ScopeTarget::name();
} catch (Error $e) {
    echo "method\n";
}
?>
--EXPECT--
read
write
method
