<?php

declare(strict_types=1);

function native_authoritative_float_return(int $value): float
{
    return $value;
}

function &native_authoritative_reference_return(int &$value): int
{
    return $value;
}

$warm = 1;
for ($i = 0; $i < 10; $i++) {
    native_authoritative_float_return($warm);
    $alias =& native_authoritative_reference_return($warm);
}

echo native_authoritative_float_return(42) === 42.0 ? "float\n" : "bad-float\n";
echo is_float(native_authoritative_float_return(9223372036854775807))
    ? "large-float\n"
    : "bad-large-float\n";

$value = 41;
$alias =& native_authoritative_reference_return($value);
$alias++;
echo $value, '|', $alias, "\n";
