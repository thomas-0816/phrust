<?php

function native_named_fixed($value): int
{
    return $value;
}

function native_named_variadic(...$values): array
{
    return $values;
}

try {
    native_named_fixed(VALUE: 1);
} catch (Error $error) {
    echo $error->getMessage(), "\n";
}

var_export(native_named_variadic(X: 1, x: 2));
echo "\n";

try {
    strlen(STRING: 'native');
} catch (Error $error) {
    echo $error->getMessage(), "\n";
}
