<?php
// Float dim keys on reads: arrays deprecate lossy implicit conversion,
// strings warn about the offset cast for every float offset
// (minimized from Zend/tests/type_coercion/float_to_int offset tests).

$a = ['a', 'b', 'c'];
var_dump($a[1.5]);
var_dump($a[2.0]);

$s = 'php';
var_dump($s[1.5]);
var_dump($s[2.0]);

set_error_handler(function ($errno, $errstr) {
    echo "handler($errno): ", $errstr, "\n";
});
var_dump($a[0.5]);
var_dump($s[0.5]);
restore_error_handler();

// Writes, isset, and unset follow the same container rules.
$w = ['a', 'b', 'c'];
$w[2.5] = 'z';
var_dump($w);
var_dump(isset($w[1.5]));
unset($w[1.5]);
var_dump($w);
$t = 'php';
$t[2.5] = 'z';
var_dump($t);
