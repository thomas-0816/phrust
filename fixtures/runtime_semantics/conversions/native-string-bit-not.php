<?php

function native_string_bit_not(): array
{
    $input = "A\x00\xffz";
    $inverted = ~$input;

    return [
        strlen($inverted),
        ord($inverted[0]),
        ord($inverted[1]),
        ord($inverted[2]),
        ord($inverted[3]),
    ];
}

$result = null;
for ($iteration = 0; $iteration < 32; $iteration++) {
    $result = native_string_bit_not();
}
var_dump($result);
