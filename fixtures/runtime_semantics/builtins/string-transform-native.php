<?php
function native_string_transform_family(string $value, int $count): string {
    return lcfirst($value)
        . "|" . ucfirst($value)
        . "|" . strrev($value)
        . "|" . str_repeat($value, $count);
}

var_dump(native_string_transform_family("aBc", 2));
var_dump(native_string_transform_family("Z", 0));

function native_string_compare_family(string $left, string $right, int $length): array {
    return [
        strcmp($left, $right),
        strcasecmp($left, $right),
        strncmp($left, $right, $length),
        strncasecmp($left, $right, $length),
    ];
}

var_dump(native_string_compare_family("Alpha", "alphaZ", 5));
