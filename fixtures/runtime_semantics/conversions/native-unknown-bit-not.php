<?php

function native_unknown_bit_not($value)
{
    return ~$value;
}

var_dump(native_unknown_bit_not(7));
$string = native_unknown_bit_not("A\x00");
var_dump(strlen($string), ord($string[0]), ord($string[1]));
var_dump(native_unknown_bit_not(3.0));
