<?php

function print_set_result(array $array): void
{
    foreach ($array as $key => $value) {
        echo $key, '=', $value, ';';
    }
    echo "\n";
}

$first = [
    'a' => 'red',
    2 => 'blue',
    'c' => 'green',
    'd' => 'red',
    'only' => 'yellow',
];
$second = [
    'a' => 'red',
    3 => 'blue',
    'x' => 'black',
];
$third = [
    'a' => 'red',
    2 => 'orange',
    'c' => 'green',
];

print_set_result(array_diff($first, $second, $third));
print_set_result(array_diff_assoc($first, $second, $third));
print_set_result(array_diff_key($first, $second, $third));
print_set_result(array_intersect($first, $second, $third));
print_set_result(array_intersect_assoc($first, $second, $third));
print_set_result(array_intersect_key($first, $second, $third));
print_set_result(array_replace($first, $second, $third));

// Value conversion is deliberately handled by the one baseline continuation
// when a set comparison is not already an exact native byte comparison.
print_set_result(array_diff([0 => 1, 1 => '2', 2 => 3], ['1'], [4]));
print_set_result(array_intersect([0 => 1, 1 => '2', 2 => 3], ['1', 2], [1, '2']));
