<?php

function set_nested_value_external(array &$input, array $path, mixed $value): void
{
    $last = count($path) - 1;
    for ($index = 0; $index < $last; ++$index) {
        $key = $path[$index];
        if (!array_key_exists($key, $input) || !is_array($input[$key])) {
            $input[$key] = array();
        }
        $input = &$input[$key];
    }
    $input[$path[$index]] = $value;
}

function get_nested_value_external(array $input, array $path): mixed
{
    foreach ($path as $key) {
        if (!array_key_exists($key, $input)) {
            return null;
        }
        $input = $input[$key];
    }
    return $input;
}
