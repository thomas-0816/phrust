<?php

final class ScalarConsumerBox
{
}

echo boolval(0) ? '1' : '0';
echo boolval('0') ? '1' : '0';
echo boolval('native') ? '1' : '0';
echo "\n";

echo floatval('12.5'), ':', floatval(null), "\n";
echo intval(7.9), ':', intval('12.5'), ':', intval(true), ':', intval(null), ':';
// The optional-base form intentionally remains one baseline-native
// continuation; the ordinary one-argument form above is the native cast.
echo intval('ff', 16), "\n";

echo '[', strval(null), ']:[', strval(false), ']:[', strval(true), ']:[';
echo strval(42), ']:[', strval(2.5), "]\n";

$box = new ScalarConsumerBox();
$closure = static fn (): null => null;
$array = [];
echo gettype(42), '/', get_debug_type(42), "\n";
echo gettype(2.5), '/', get_debug_type(2.5), "\n";
echo gettype('native'), '/', get_debug_type('native'), "\n";
echo gettype($array), '/', get_debug_type($array), "\n";
echo gettype($box), '/', get_debug_type($box), "\n";
echo gettype($closure), '/', get_debug_type($closure), "\n";

$value = '8.5';
$reference =& $value;
echo floatval($reference), ':', intval($reference), ':[', strval($reference), "]\n";
