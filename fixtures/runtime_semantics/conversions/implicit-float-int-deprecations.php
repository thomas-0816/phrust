<?php
// Implicit float→int conversions in int-only operator contexts deprecate
// with the reference wording; integral in-range floats stay silent
// (minimized from Zend/tests/type_coercion/float_to_int/warnings_*.phpt).

var_dump(~1.5);
var_dump(1.5 | 3);
var_dump(1.5 & 3);
var_dump(1.5 ^ 3);
var_dump(1.5 << 3);
var_dump(3 << 1.5);
var_dump(6.5 % 2);
var_dump(9 % 2.5);
var_dump("1.5" | 3);
var_dump("2.5" % 2);

// Compatible values convert silently.
var_dump(2.0 | 1);
var_dump(~4.0);
var_dump("8.0" | 1);

// The deprecation reaches a user error handler.
set_error_handler(function ($errno, $errstr) {
    echo "handler($errno): ", $errstr, "\n";
});
var_dump(1.5 | 3);
restore_error_handler();
