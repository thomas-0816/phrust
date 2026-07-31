<?php

function native_return_int($value): int {
    return $value;
}

function native_return_float($value): float {
    return $value;
}

function native_return_string($value): string {
    return $value;
}

function native_return_bool($value): bool {
    return $value;
}

function native_return_nullable_int($value): ?int {
    return $value;
}

function &native_return_reference_int(): int {
    static $value = "43";
    return $value;
}

function &native_return_reference_int_error(): int {
    static $value = "not numeric";
    return $value;
}

echo gettype(native_return_int("42")), ":", native_return_int("42"), "|";
echo gettype(native_return_float("3.5")), ":", native_return_float("3.5"), "|";
echo gettype(native_return_string(17)), ":", native_return_string(17), "|";
echo gettype(native_return_bool("0")), ":", (native_return_bool("0") ? "1" : "0"), "\n";
echo gettype(native_return_nullable_int(null)), ":null|";
echo gettype(native_return_nullable_int("7")), ":", native_return_nullable_int("7"), "\n";
$reference =& native_return_reference_int();
echo gettype($reference), ":", $reference, "|";
$reference = 8;
$same_reference =& native_return_reference_int();
echo gettype($same_reference), ":", $same_reference, "\n";

try {
    native_return_int("not numeric");
} catch (TypeError $error) {
    echo get_class($error), "\n";
}

try {
    $invalid_reference =& native_return_reference_int_error();
} catch (TypeError $error) {
    echo get_class($error), "\n";
}
