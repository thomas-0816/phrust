--TEST--
Generated scalar.methods: float object methods
--DESCRIPTION--
module: scalar.methods
generated timestamp: 20260707T120000Z
generator version: phpt-scalar.methods-v1
reason: Lock in the float scalar object methods (round, ceil, floor, abs)
--FILE--
<?php
echo (3.14159)->round(2) . "\n";
echo (3.14159)->ceil() . "\n";
echo (3.14159)->floor() . "\n";
echo ((-3.5)->abs()) . "\n";
?>
--EXPECT--
3.14
4
3
3.5
