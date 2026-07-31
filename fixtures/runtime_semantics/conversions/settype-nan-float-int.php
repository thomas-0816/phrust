<?php
// Regression fixture for settype() and float→int conversion parity
// (minimized from Zend/tests/type_coercion/settype/*.phpt and
// Zend/tests/type_coercion/float_to_int behavior).

$v = "12abc";
var_dump(settype($v, "integer"), $v);

$w = "5";
var_dump(settype($w, "Integer"), $w);

try {
    settype($w, "nonsense");
} catch (\ValueError $e) {
    echo get_class($e), ": ", $e->getMessage(), "\n";
}

try {
    settype($w, "resource");
} catch (\ValueError $e) {
    echo get_class($e), ": ", $e->getMessage(), "\n";
}
var_dump($w);

$n = 1;
settype($n, "null");
var_dump($n);

$o = "text";
settype($o, "object");
var_dump($o instanceof stdClass, $o->scalar);

$a = [1, 2];
settype($a, "string");
var_dump($a);

// Float→int casts use Zend's modular conversion without diagnostics.
var_dump((int) fdiv(0, 0));
var_dump((int) INF);
var_dump((int) -INF);
var_dump((int) 1e30);
var_dump((int) 3.7);

// Native strings use PHP's numeric-prefix rules for scalar casts.
foreach (["12abc", "  -3.5x", "abc", ""] as $numericText) {
    var_dump((int) $numericText, (float) $numericText);
}

// NAN coercions are ordinary conversions; bool(NAN) is true.
$nan = fdiv(0, 0);
var_dump((string) $nan);
settype($nan, "bool");
var_dump($nan);
$nan2 = fdiv(0, 0);
var_dump((array) $nan2);

// A user error handler is not invoked by ordinary NAN conversion.
set_error_handler(function ($errno, $errstr) {
    global $nan3;
    $nan3 = null;
    echo "handler: ", $errstr, "\n";
});
$nan3 = fdiv(0, 0);
settype($nan3, "array");
var_dump($nan3);
restore_error_handler();
