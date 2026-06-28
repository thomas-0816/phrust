--TEST--
standard.math: generated core math edge cases
--DESCRIPTION--
Generated standard.math baseline covering constants, base conversion, fmod NaN,
and integer rounding modes from ext/standard math behavior.
--FILE--
<?php
var_dump(pi() === M_PI);
var_dump(dechex(255));
var_dump(decoct(64));
var_dump(bindec("1010"));
var_dump(octdec("10"));
var_dump(hexdec("ff"));
var_dump(base_convert("ff", 16, 2));
var_dump(is_nan(fmod(1, 0)));
var_dump(round(2.5, 0, PHP_ROUND_HALF_DOWN));
var_dump(round(2.5, 0, PHP_ROUND_HALF_EVEN));
var_dump(round(2.5, 0, PHP_ROUND_HALF_ODD));
?>
--EXPECT--
bool(true)
string(2) "ff"
string(3) "100"
int(10)
int(8)
int(255)
string(8) "11111111"
bool(true)
float(2)
float(2)
float(3)
