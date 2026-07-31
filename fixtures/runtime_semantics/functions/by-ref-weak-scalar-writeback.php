<?php
function coerce_ref(int &$value): void {
    echo gettype($value), ":", $value, "|";
}

function coerce_float_ref(float &$value): void {
    echo gettype($value), ":", $value, "|";
}

function coerce_string_ref(string &$value): void {
    echo gettype($value), ":", $value, "|";
}

function coerce_bool_ref(bool &$value): void {
    echo gettype($value), ":", ($value ? "1" : "0"), "|";
}

$integer = "42";
$float = "3.5";
$string = 17;
$bool = "0";
coerce_ref($integer);
coerce_float_ref($float);
coerce_string_ref($string);
coerce_bool_ref($bool);
echo gettype($integer), ":", $integer, "|";
echo gettype($float), ":", $float, "|";
echo gettype($string), ":", $string, "|";
echo gettype($bool), ":", ($bool ? "1" : "0");
