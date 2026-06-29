--TEST--
closure.core: late static binding captured by closure
--DESCRIPTION--
module: closure.core
generated timestamp: 20260629T000000Z
generator version: closure-core-runtime-v1
reason: closure core dashboard covers static:: binding inside returned closures
oracle: Reference PHP 8.5.7
--FILE--
<?php
class ClosureCoreBase {
    public static function make(): Closure {
        return function (): void {
            echo static::class, "\n";
        };
    }
}

class ClosureCoreChild extends ClosureCoreBase {
}

$fn = ClosureCoreChild::make();
$fn();
?>
--EXPECT--
ClosureCoreChild
