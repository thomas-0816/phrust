<?php

function native_float_bit_not(): array
{
    $positive = 3.0;
    $negative = -3.0;
    $zero = -0.0;

    return [
        ~$positive,
        ~$negative,
        ~$zero,
        ~(17.0),
    ];
}

$result = null;
for ($iteration = 0; $iteration < 32; $iteration++) {
    $result = native_float_bit_not();
}
var_dump($result);
