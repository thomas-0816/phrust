<?php

echo pow(2, 10), ':', pow(2, -2), ':', pow(9, 0.5), ':', pow('2', '3'), "\n";
echo intdiv(7, 3), ':', intdiv(-7, 3), "\n";

try {
    intdiv(1, 0);
} catch (Throwable) {
    echo "division-zero\n";
}

echo round(2.5), ':';
echo round(2.5, 0, 2), ':';
echo round(2.5, 0, 3), ':';
echo round(2.5, 0, 4), ':';
echo round(2.1, 0, 5), ':';
echo round(-2.1, 0, 6), ':';
echo round(2.9, 0, 7), ':';
echo round(2.1, 0, 8), "\n";

try {
    round(1.25, 1, 9);
} catch (Throwable) {
    echo "invalid-round-mode\n";
}

// Numeric-string values preserve coercion through the call's one baseline
// continuation. Ordinary int/float values and validated integer modes above
// remain native.
echo round('2.55', 1), "\n";

$base = 3;
$exponent = 4;
$dividend = 17;
$divisor = 5;
$rounded = 1.25;
$base_ref =& $base;
$exponent_ref =& $exponent;
$dividend_ref =& $dividend;
$divisor_ref =& $divisor;
$rounded_ref =& $rounded;
echo pow($base_ref, $exponent_ref), ':';
echo intdiv($dividend_ref, $divisor_ref), ':';
echo round($rounded_ref, 1), "\n";
