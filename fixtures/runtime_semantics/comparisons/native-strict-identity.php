<?php
function native_strict_identity(mixed $left, mixed $right): array {
    return [$left === $right, $left !== $right];
}

function native_loose_identity(mixed $left, mixed $right): array {
    return [$left == $right, $left != $right];
}

$same_a = "same";
$same_b = substr("xsame", 1);
$object = new stdClass();

foreach ([
    [null, null],
    [null, false],
    [1, 1],
    [1, true],
    [$same_a, $same_b],
    ["same", "different"],
    [0.0, -0.0],
    [NAN, NAN],
    [$object, $object],
    [$object, new stdClass()],
    [[1, "a"], [1, "a"]],
    [[1, "a"], [1, "b"]],
] as [$left, $right]) {
    var_dump(native_strict_identity($left, $right));
}

$value = "same";
$reference =& $value;
var_dump(native_strict_identity($reference, $same_b));

foreach ([
    [10, 10],
    [10, 11],
    ["alpha", "alpha"],
    ["alpha", "beta"],
    [10, "alpha"],
    [10, "10"],
    [false, 0],
] as [$left, $right]) {
    var_dump(native_loose_identity($left, $right));
}
