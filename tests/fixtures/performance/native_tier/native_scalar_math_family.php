<?php

function native_math_abs($value)
{
    return abs($value);
}

function native_math_ceil($value)
{
    return ceil($value);
}

function native_math_floor($value)
{
    return floor($value);
}

function native_math_sqrt($value)
{
    return sqrt($value);
}

function native_math_fdiv($left, $right)
{
    return fdiv($left, $right);
}

function native_math_fmod($left, $right)
{
    return fmod($left, $right);
}

function native_math_is_finite($value)
{
    return is_finite($value);
}

function native_math_is_infinite($value)
{
    return is_infinite($value);
}

function native_math_is_nan($value)
{
    return is_nan($value);
}

function native_math_pi()
{
    return pi();
}

echo native_math_abs(-7), "\n";
echo native_math_abs(PHP_INT_MIN), "\n";
echo native_math_abs('2.5'), "\n";
echo native_math_ceil(7.25), "\n";
echo native_math_floor(7.75), "\n";
echo native_math_sqrt(81), "\n";
echo native_math_fdiv(7.5, 2), "\n";
echo native_math_fmod(7.5, 2), "\n";
echo native_math_is_finite(7.5), "\n";
echo native_math_is_infinite(INF), "\n";
echo native_math_is_nan(NAN), "\n";
echo native_math_pi(), "\n";
