--TEST--
Generated objects.core: protected method external call is Error
--DESCRIPTION--
module: objects.core
generated timestamp: 20260627T000000Z
generator version: phpt-objects-visibility-v1
reason: protected method external visibility error
--FILE--
<?php
class Secret {
    protected function hidden() {
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
Call to protected method Secret::hidden() from global scope
