<?php

function native_array_default_call(array $values = []): int
{
    $values[] = 'native';
    return count($values);
}

function native_nested_array_default_call(
    array $values = ['seed' => ['value' => 1]],
): int {
    ++$values['seed']['value'];
    return $values['seed']['value'];
}

function native_reference_array_default_call(array &$values = []): int
{
    $values[] = 'reference';
    return count($values);
}

echo native_array_default_call(), "\n";
echo native_array_default_call(), "\n";
echo native_array_default_call(['existing']), "\n";
echo native_nested_array_default_call(), "\n";
echo native_nested_array_default_call(), "\n";
echo native_reference_array_default_call(), "\n";
echo native_reference_array_default_call(), "\n";
