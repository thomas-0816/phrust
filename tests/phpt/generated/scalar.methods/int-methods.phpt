--TEST--
Generated scalar.methods: int object methods
--DESCRIPTION--
module: scalar.methods
generated timestamp: 20260707T120000Z
generator version: phpt-scalar.methods-v1
reason: Lock in the int scalar object methods (abs, pow, clamp)
--FILE--
<?php
echo (-5)->abs() . "\n";
echo (3)->pow(2) . "\n";
echo (5)->clamp(1, 3) . "\n";
echo (1)->clamp(2, 5) . "\n";
echo "hello"->length()->pow(2) . "\n";
?>
--EXPECT--
5
9
3
2
25
