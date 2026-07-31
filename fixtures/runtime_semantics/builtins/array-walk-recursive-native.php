<?php
// runtime-semantics: category=builtins expect=pass php_ref_required=1

function native_recursive_walk_callback(int &$value, int $key, int $delta): void
{
    $value += $key + $delta;
}

function native_recursive_walk(array $values, int $delta): array
{
    array_walk_recursive($values, 'native_recursive_walk_callback', $delta);
    return $values;
}

$source = [
    2 => 5,
    4 => [
        6 => 7,
        8 => [
            10 => 11,
        ],
    ],
];
$alias = $source;
$walked = native_recursive_walk($source, 3);

var_dump($source, $alias, $walked);
