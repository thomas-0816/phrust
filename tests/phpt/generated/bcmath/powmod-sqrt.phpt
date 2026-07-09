--TEST--
bcmath powmod and sqrt compatibility
--SKIPIF--
<?php if (!extension_loaded("bcmath")) die("skip bcmath extension not loaded"); ?>
--FILE--
<?php
echo bcsqrt("2", 4), "\n";
echo bcsqrt("4", 5), "\n";
echo bcsqrt("0.0004", 4), "\n";
echo bcsqrt("-0.00", 2), "\n";
echo bcpowmod("2", "10", "1000"), "\n";
echo bcpowmod("2", "10", "1000", 2), "\n";
echo bcpowmod("-2", "5", "7"), "\n";
echo bcpowmod("5", "0", "-1", 3), "\n";

foreach (
    [
        fn() => bcsqrt("-1", 2),
        fn() => bcpowmod("2.01", "3", "7", 2),
        fn() => bcpowmod("2", "3.01", "7", 2),
        fn() => bcpowmod("2", "-1", "7", 2),
        fn() => bcpowmod("2", "3", "0", 2),
    ] as $callback
) {
    try {
        $callback();
    } catch (Throwable $e) {
        echo $e->getMessage(), "\n";
    }
}
?>
--EXPECT--
1.4142
2.00000
0.0200
0.00
24
24.00
-4
0.000
bcsqrt(): Argument #1 ($num) must be greater than or equal to 0
bcpowmod(): Argument #1 ($num) cannot have a fractional part
bcpowmod(): Argument #2 ($exponent) cannot have a fractional part
bcpowmod(): Argument #2 ($exponent) must be greater than or equal to 0
Modulo by zero
