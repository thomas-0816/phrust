<?php

function native_scalar_int_cast($value): int
{
    return (int) $value;
}

function native_scalar_float_cast($value): float
{
    return (float) $value;
}

function native_scalar_add($lhs, $rhs)
{
    return $lhs + $rhs;
}

function native_scalar_sub($lhs, $rhs)
{
    return $lhs - $rhs;
}

function native_scalar_mul($lhs, $rhs)
{
    return $lhs * $rhs;
}

function native_scalar_div($lhs, $rhs)
{
    return $lhs / $rhs;
}

function native_scalar_pow($lhs, $rhs)
{
    return $lhs ** $rhs;
}

function native_scalar_mod($lhs, $rhs)
{
    return $lhs % $rhs;
}

function native_scalar_bit_and($lhs, $rhs)
{
    return $lhs & $rhs;
}

function native_scalar_bit_or($lhs, $rhs)
{
    return $lhs | $rhs;
}

function native_scalar_bit_xor($lhs, $rhs)
{
    return $lhs ^ $rhs;
}

function native_scalar_shift_left($lhs, $rhs)
{
    return $lhs << $rhs;
}

function native_scalar_shift_right($lhs, $rhs)
{
    return $lhs >> $rhs;
}

function native_scalar_plus($value)
{
    return +$value;
}

function native_scalar_minus($value)
{
    return -$value;
}

echo native_scalar_int_cast(true), "\n";
echo native_scalar_int_cast(false), "\n";
echo native_scalar_int_cast(null), "\n";
echo native_scalar_int_cast(3.75), "\n";
echo native_scalar_int_cast(-2.75), "\n";
echo native_scalar_int_cast('42tail'), "\n";
echo native_scalar_int_cast('plain'), "\n";
echo native_scalar_float_cast(true), "\n";
echo native_scalar_float_cast(false), "\n";
echo native_scalar_float_cast(null), "\n";
echo native_scalar_float_cast(8), "\n";
echo native_scalar_float_cast(-3), "\n";
echo native_scalar_float_cast('2.5tail'), "\n";
echo native_scalar_float_cast('plain'), "\n";
echo native_scalar_add('2', '3'), "\n";
echo native_scalar_add('2.5', '3'), "\n";
echo native_scalar_sub('7', '2'), "\n";
echo native_scalar_mul('6', '7'), "\n";
echo native_scalar_div('6', '3') === 2 ? "integer-div\n" : "wrong-div\n";
echo native_scalar_div('7', '2') === 3.5 ? "float-div\n" : "wrong-div\n";
echo native_scalar_pow('3', '4'), "\n";
echo native_scalar_pow('9', '0.5') === 3.0 ? "float-pow\n" : "wrong-pow\n";
echo native_scalar_mod('5', '2'), "\n";
echo native_scalar_shift_left('8', '2'), "\n";
echo native_scalar_shift_right('-8', '2'), "\n";
echo native_scalar_bit_and(8, '3'), "\n";
echo native_scalar_bit_and('A', 'a') === 'A' ? "string-and\n" : "wrong-and\n";
echo native_scalar_bit_or('A', 'bc') === 'cc' ? "string-or\n" : "wrong-or\n";
echo native_scalar_bit_xor('AB', 'a') === ' ' ? "string-xor\n" : "wrong-xor\n";
echo native_scalar_plus('7'), "\n";
echo native_scalar_minus('2.5'), "\n";
echo native_scalar_add(true, null), "\n";
echo native_scalar_add('9007199254740993', '0') === 9007199254740993 ? "exact\n" : "lost\n";
