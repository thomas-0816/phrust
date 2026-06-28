--TEST--
Generated objects.core: catch(Error) catches visibility errors
--DESCRIPTION--
module: objects.core
generated timestamp: 20260627T000000Z
generator version: phpt-objects-visibility-v1
reason: Prompt 14.4 catchable Error routing for visibility errors
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
    echo "caught:", get_class($e), ":", $e->getMessage(), "\n";
}
?>
--EXPECT--
caught:Error:Call to private method Secret::hidden() from global scope
