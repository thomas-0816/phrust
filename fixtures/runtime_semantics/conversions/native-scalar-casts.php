<?php

function native_scalar_casts(): array
{
    $integer = 17;
    $negative = -23;
    $truth = true;
    $falsehood = false;
    $nothing = null;
    $fraction = -19.75;
    $negativeZero = -0.0;
    $emptyArray = [];
    $filledArray = [1];
    $integerReference = &$integer;

    return [
        (float) $integer,
        (float) $negative,
        (float) $truth,
        (float) $falsehood,
        (float) $nothing,
        (float) $fraction,
        (float) $negativeZero,
        (int) $fraction,
        (int) 19.75,
        (bool) $fraction,
        (bool) $negativeZero,
        (int) $emptyArray,
        (int) $filledArray,
        (float) $emptyArray,
        (float) $filledArray,
        (string) $integer,
        (string) $fraction,
        (string) $truth,
        (string) $falsehood,
        (string) $nothing,
        (string) $integerReference,
    ];
}

$result = null;
for ($iteration = 0; $iteration < 32; $iteration++) {
    $result = native_scalar_casts();
}
var_dump($result);
