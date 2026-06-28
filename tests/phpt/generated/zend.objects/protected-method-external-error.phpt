--TEST--
Generated zend.objects: protected method external call is Error
--DESCRIPTION--
module: zend.objects
generated timestamp: 20260627T000000Z
generator version: phpt-objects-visibility-v1
reason: Prompt 14.4 protected method external visibility error
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
