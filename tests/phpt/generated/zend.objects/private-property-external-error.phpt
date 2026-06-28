--TEST--
Generated zend.objects: private property external access is Error
--DESCRIPTION--
module: zend.objects
generated timestamp: 20260627T000000Z
generator version: phpt-objects-visibility-v1
reason: Prompt 14.4 private property external visibility error
--FILE--
<?php
class Secret {
    private $hidden = 1;
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
read:Cannot access private property Secret::$hidden
write:Cannot access private property Secret::$hidden
