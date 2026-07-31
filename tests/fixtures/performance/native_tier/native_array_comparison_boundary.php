<?php

function native_array_identical($left, $right): bool
{
    return $left === $right;
}

function native_array_equal($left, $right): bool
{
    return $left == $right;
}

function native_array_order($left, $right): int
{
    return $left <=> $right;
}

$ordered = [
    'first' => 1,
    'nested' => [2, '3'],
];
$same = [
    'first' => 1,
    'nested' => [2, '3'],
];
$reordered = [
    'nested' => [2, '3'],
    'first' => 1,
];
$looselyEqual = [
    'first' => 1.0,
    'nested' => ['2', 3],
];

echo native_array_identical($ordered, $same) ? "strict-same\n" : "strict-wrong\n";
echo native_array_identical($ordered, $reordered) ? "strict-wrong\n" : "strict-order\n";
echo native_array_equal($ordered, $reordered) ? "loose-order\n" : "loose-wrong\n";
echo native_array_equal($ordered, $looselyEqual) ? "loose-nested\n" : "loose-wrong\n";
echo native_array_order([1, 2], [1, 3]), "\n";
echo native_array_order(['b' => 1], ['a' => 1]), "\n";
echo native_array_order([], 0), "\n";
