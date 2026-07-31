<?php

function native_loose_equal($left, $right): bool
{
    return $left == $right;
}

function native_spaceship($left, $right): int
{
    return $left <=> $right;
}

function native_array_cast($value): array
{
    return (array) $value;
}

function native_reference_is_int(&$value): bool
{
    return is_int($value);
}

function native_reference_is_float(&$value): bool
{
    return is_float($value);
}

echo native_loose_equal(null, '0') ? "T\n" : "F\n";
echo native_loose_equal(null, '') ? "T\n" : "F\n";
echo native_loose_equal(false, '0') ? "T\n" : "F\n";
echo native_spaceship(null, -1), "\n";
echo native_spaceship(null, '0'), "\n";
echo native_spaceship(3.5, 2), "\n";
echo native_spaceship(2, 3.5), "\n";
echo native_spaceship('abc', 'abd'), "\n";
echo native_spaceship('1', '2'), "\n";
echo native_loose_equal(9007199254740992, 9007199254740993) ? "T\n" : "F\n";
echo native_spaceship(9007199254740992, 9007199254740993), "\n";
echo native_loose_equal('001', '1') ? "T\n" : "F\n";
echo native_spaceship('001', '1'), "\n";
echo native_loose_equal('1e3', '1000') ? "T\n" : "F\n";
echo native_spaceship('1e3', '1000'), "\n";
echo native_loose_equal('9007199254740993', '9007199254740992') ? "T\n" : "F\n";
echo native_spaceship('9007199254740993', '9007199254740992'), "\n";
echo native_loose_equal(42, '42abc') ? "T\n" : "F\n";
echo native_spaceship(42, '42abc'), "\n";
echo native_loose_equal('42abc', 42) ? "T\n" : "F\n";
echo native_spaceship('42abc', 42), "\n";
echo native_loose_equal(0, 'abc') ? "T\n" : "F\n";
echo native_spaceship(0, 'abc'), "\n";
echo native_loose_equal('abc', 0) ? "T\n" : "F\n";
echo native_spaceship('abc', 0), "\n";
echo native_loose_equal(10, '2foo') ? "T\n" : "F\n";
echo native_spaceship(10, '2foo'), "\n";
echo native_loose_equal('2foo', 10) ? "T\n" : "F\n";
echo native_spaceship('2foo', 10), "\n";
echo native_loose_equal(INF, 'INF') ? "T\n" : "F\n";
echo native_spaceship(INF, 'INF'), "\n";
echo native_loose_equal(NAN, 'NAN') ? "T\n" : "F\n";
echo native_spaceship(NAN, 'NAN'), "\n";

$lookup = ['9007199254740992', '9007199254740993'];
echo in_array(1, ['001'], false) ? "T\n" : "F\n";
echo array_search('9007199254740993', $lookup, false) === 1 ? "T\n" : "F\n";
echo in_array(10, ['2foo'], false) ? "T\n" : "F\n";
echo in_array(NAN, ['NAN'], false) ? "T\n" : "F\n";

$empty = native_array_cast(null);
$boolean = native_array_cast(true);
$float = native_array_cast(2.5);
$existing = native_array_cast(['kept']);
echo count($empty), "\n";
echo count($boolean), ':', $boolean[0] === true ? 'T' : 'F', "\n";
echo count($float), ':', $float[0] === 2.5 ? 'T' : 'F', "\n";
echo count($existing), ':', $existing[0], "\n";

$value = 42;
$reference =& $value;
echo native_reference_is_int($reference) ? "T\n" : "F\n";
$value = 2.5;
echo native_reference_is_float($reference) ? "T\n" : "F\n";
