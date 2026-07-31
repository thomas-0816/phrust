<?php
// runtime-fixture: kind=valid expected_stdout="int(0)\nint(0)\nint(0)\nint(7766279631452241920)\nint(-7766279631452241920)\nint(1)\nbool(true)\narray(1) {\n  [0]=>\n  float(NAN)\n}\nbool(true)\nbool(true)\nbool(true)\narray(1) {\n  [0]=>\n  float(NAN)\n}\n"
function explicit_float_to_int_range(float $value): int
{
    return (int) $value;
}

foreach ([INF, -INF, NAN, 1.0e20, -1.0e20, 1.5] as $value) {
    var_dump(explicit_float_to_int_range($value));
}

var_dump((bool) NAN);
var_dump((array) NAN);

$nanBool = NAN;
var_dump(settype($nanBool, 'boolean'));
var_dump($nanBool);
$nanArray = NAN;
var_dump(settype($nanArray, 'array'));
var_dump($nanArray);
