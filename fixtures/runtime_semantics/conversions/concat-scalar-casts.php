<?php

function native_scalar_string_operations(mixed $value): array
{
    return [
        "x" . true,
        false . "y",
        null . "z",
        -42 . "!",
        1.7000000000000002 . "?",
        $value . ":dynamic",
        bin2hex("\xf0\x0f" & "\xcc\xaa"),
        bin2hex("\xf0\x0f" | "\xcc\xaa\x55"),
        bin2hex("\xf0\x0f" ^ "\xcc\xaa\x55"),
        bin2hex(~"\x00\xa5\xff"),
        (string) $value,
    ];
}

function native_float_string(float $value): string
{
    return (string) $value;
}

$result = null;
for ($iteration = 0; $iteration < 32; $iteration++) {
    $result = native_scalar_string_operations(42);
    $result[] = native_float_string(1.7000000000000002);
}
var_dump($result);
