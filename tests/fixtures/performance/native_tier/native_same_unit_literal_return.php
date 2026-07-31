<?php

function native_same_unit_identity($value)
{
    return $value;
}

function native_same_unit_forward()
{
    return native_same_unit_identity('native-literal');
}

function native_same_unit_forward_colliding_integer()
{
    return native_same_unit_identity(0x7ff1000000000000);
}

echo native_same_unit_forward(), "\n";
echo native_same_unit_forward_colliding_integer() === 0x7ff1000000000000
    ? "native-integer\n"
    : "wrong-integer\n";
