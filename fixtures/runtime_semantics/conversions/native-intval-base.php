<?php
// runtime-semantics: expect=pass

function native_intval_base(mixed $value, mixed $base): int
{
    return intval($value, $base);
}

$result = null;
for ($iteration = 0; $iteration < 32; $iteration++) {
    $result = [
        native_intval_base("ff", 16),
        native_intval_base("-101", 2),
        native_intval_base("077", 0),
        native_intval_base("0x2a", 0),
        native_intval_base("1e2", 10),
        native_intval_base(19.75, 2),
        native_intval_base([], 16),
        native_intval_base([1], 16),
    ];
}

var_dump($result);
