<?php

function native_division(): array
{
    $first = 7 / 2;
    $second = 9.0 / 4;
    $third = -5 / 2;
    $sum = 1.5 + 2;
    $difference = 5 - 1.25;
    $product = 2.5 * 4;
    $overflow = 9223372036854775807 + 1;
    $power = 2 ** 10;
    $negativePower = 2 ** -3;
    $floatPower = 2.5 ** 3;
    $overflowPower = 2 ** 63;
    $unarySource = 2.5;
    $unaryPlus = +$unarySource;
    $unaryMinus = -$unarySource;
    $minimum = -9223372036854775807 - 1;
    $minimumNegated = -$minimum;

    return [
        $first,
        $second,
        $third,
        $first / 2,
        $sum,
        $difference,
        $product,
        $sum * 2,
        sprintf('%.0f', $overflow),
        $power,
        $negativePower,
        $floatPower,
        sprintf('%.0f', $overflowPower),
        $unaryPlus,
        $unaryMinus,
        sprintf('%.0f', $minimumNegated),
    ];
}

$result = null;
for ($iteration = 0; $iteration < 32; $iteration++) {
    $result = native_division();
}
var_dump($result);
