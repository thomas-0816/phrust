<?php

function native_scalar_concat($left, $right): string
{
    return $left . '|' . $right;
}

function native_scalar_cast($value): string
{
    return (string) $value;
}

function native_scalar_echo($value): void
{
    echo $value, "\n";
}

echo native_scalar_concat(42, true), "\n";
echo native_scalar_concat(null, 3.5), "\n";
echo '[' . native_scalar_cast(false) . ']', "\n";
echo '[' . native_scalar_cast(null) . ']', "\n";
echo native_scalar_cast(17), "\n";
echo native_scalar_cast(2.25), "\n";
native_scalar_echo(19);
native_scalar_echo(true);
native_scalar_echo(false);
native_scalar_echo(null);
native_scalar_echo(4.5);
