<?php
// runtime-semantics: category=conversions expect=pass
// Exact native Binary operation family: scalars, strings, arrays, references,
// integer overflow, and floating-point results.

var_dump("10" + "3");
var_dump("10" - "3");
var_dump("10" * "3");
var_dump("8" / "2");
var_dump("7" / "2");
var_dump("5" % "2");
var_dump("8" << "2");
var_dump("-8" >> "2");
var_dump(10 & 6);
var_dump(10 | 5);
var_dump(10 ^ 3);

var_dump("AB" & "a");
var_dump("A" | "bc");
var_dump("AB" ^ "a");

var_dump(2 ** 10);
var_dump(9 ** 0.5);
printf("%.0f\n", PHP_INT_MAX + 1);

var_dump(+"12");
var_dump(-"2.5");
printf("%.0f\n", -PHP_INT_MIN);
var_dump(~1);
var_dump(~1.0);
var_dump(~"AB");

$left = "native";
$alias =& $left;
var_dump($alias . ":" . 42 . true . null . 1.5);

var_dump(
    [0 => "left", "x" => 1]
    + [0 => "right", "y" => 2]
);
