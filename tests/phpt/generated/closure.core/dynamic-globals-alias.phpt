--TEST--
closure.core: dynamic globals table binding
--DESCRIPTION--
module: closure.core
generated timestamp: 20260629T000000Z
generator version: closure-core-runtime-v1
reason: closure core dashboard covers $GLOBALS writes and dynamic global binding
oracle: Reference PHP 8.5.7
--FILE--
<?php
$foo = "local";
$GLOBALS["foo"] = "global";
echo $foo, ":", $GLOBALS["foo"], "\n";

function set_dynamic_global(): void {
    $name = "closure_core_dynamic";
    $GLOBALS[$name] = "set";
    global $closure_core_dynamic;
    echo $closure_core_dynamic, "\n";
}

set_dynamic_global();
echo $closure_core_dynamic, "\n";
?>
--EXPECT--
global:global
set
set
