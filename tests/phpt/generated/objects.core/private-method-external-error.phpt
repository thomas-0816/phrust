--TEST--
Generated objects.core: private method external call is Error
--DESCRIPTION--
module: objects.core
generated timestamp: 20260628T000000Z
generator version: phpt-objects-core-v1
reason: Branch 1 object-core private method external visibility error
--FILE--
<?php
class Secret {
    private function hidden() {
        return "hidden";
    }
}

try {
    echo (new Secret())->hidden();
} catch (Error $e) {
    echo $e->getMessage(), "\n";
}
?>
--EXPECT--
Call to private method Secret::hidden() from global scope
