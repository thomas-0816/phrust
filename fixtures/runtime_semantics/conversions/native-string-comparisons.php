<?php

function native_string_comparisons(): array
{
    $apple = "apple";
    $banana = "banana";
    $same = "apple";
    $numericLow = "2";
    $numericHigh = "10";

    return [
        $apple < $banana,
        $apple <= $same,
        $banana > $apple,
        $banana >= $same,
        $apple <=> $banana,
        $banana <=> $apple,
        $apple <=> $same,
        $numericLow > $numericHigh,
        $numericLow <=> $numericHigh,
    ];
}

$result = null;
for ($iteration = 0; $iteration < 32; $iteration++) {
    $result = native_string_comparisons();
}
var_dump($result);
