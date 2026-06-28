--TEST--
Generated objects.core: protected property external access is Error
--DESCRIPTION--
module: objects.core
generated timestamp: 20260628T000000Z
generator version: phpt-objects-core-v1
reason: Branch 1 object-core protected property external visibility error
--FILE--
<?php
class Secret {
    protected $hidden = 1;
}

$secret = new Secret();
try {
    echo $secret->hidden;
} catch (Error $e) {
    echo "read:", $e->getMessage(), "\n";
}

try {
    $secret->hidden = 2;
} catch (Error $e) {
    echo "write:", $e->getMessage(), "\n";
}
?>
--EXPECT--
read:Cannot access protected property Secret::$hidden
write:Cannot access protected property Secret::$hidden
