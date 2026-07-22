<?php

function native_array_casts(): array
{
    $nothing = null;
    $integer = 7;
    $float = 2.5;
    $string = "native";
    $source = [1, 2];

    $empty = (array) $nothing;
    $integerArray = (array) $integer;
    $floatArray = (array) $float;
    $stringArray = (array) $string;
    $copy = (array) $source;
    $copy[] = 3;

    return [$empty, $integerArray, $floatArray, $stringArray, $source, $copy];
}

$result = null;
for ($iteration = 0; $iteration < 32; $iteration++) {
    $result = native_array_casts();
}
var_dump($result);
