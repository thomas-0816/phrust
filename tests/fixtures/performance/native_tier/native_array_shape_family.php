<?php

function print_array_shape(array $array): void
{
    foreach ($array as $key => $value) {
        if (is_array($value)) {
            echo $key, '=[';
            foreach ($value as $innerKey => $innerValue) {
                echo $innerKey, '=', $innerValue, ';';
            }
            echo '];';
        } else {
            echo $key, '=', $value, ';';
        }
    }
    echo "\n";
}

print_array_shape(range(2, 8, 2));
print_array_shape(range(8, 2, 3));
print_array_shape(range(0.5, 1.5, 0.5));

$mixedKeys = ['name' => 'alpha', 5 => 'beta'];
print_array_shape(array_pad($mixedKeys, 4, 'right'));
print_array_shape(array_pad($mixedKeys, -4, 'left'));
print_array_shape(array_pad($mixedKeys, 1, 'unused'));

print_array_shape(array_chunk(['name' => 'alpha', 5 => 'beta', 'tail' => 'gamma'], 2));
print_array_shape(array_chunk(
    ['name' => 'alpha', 5 => 'beta', 'tail' => 'gamma'],
    2,
    true
));

$rows = [
    ['id' => 'a', 'label' => 'first'],
    ['id' => 'b'],
    ['id' => 'c', 'label' => 'third'],
];
print_array_shape(array_column($rows, 'label'));
print_array_shape(array_column($rows, null));
print_array_shape(array_column($rows, 'label', 'id'));

$indexedRows = [
    ['id' => 'x', 'value' => 'first'],
    ['id' => 'x', 'value' => 'second'],
    ['value' => 'missing'],
    ['id' => 5, 'value' => 'five'],
    ['value' => 'tail'],
];
print_array_shape(array_column($indexedRows, 'value', 'id'));

print_array_shape(array_unique([
    'first' => 'alpha',
    4 => 'beta',
    'again' => 'alpha',
    'last' => 'gamma',
]));
print_array_shape(array_unique([1, '1', 2, '02']));
