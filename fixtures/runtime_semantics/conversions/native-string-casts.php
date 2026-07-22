<?php

function native_string_casts(): array
{
    $nothing = null;
    $falsehood = false;
    $truth = true;
    $zero = 0;
    $positive = 1234567890;
    $negative = -987654321;
    $minimum = -9223372036854775807 - 1;
    $existing = "native";

    return [
        (string) $nothing,
        (string) $falsehood,
        (string) $truth,
        (string) $zero,
        (string) $positive,
        (string) $negative,
        (string) $minimum,
        (string) $existing,
    ];
}

$result = null;
for ($iteration = 0; $iteration < 32; $iteration++) {
    $result = native_string_casts();
}
var_dump($result);
