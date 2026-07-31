<?php

function native_weak_int(int $value): string {
    return gettype($value) . ":" . $value;
}

function native_weak_float(float $value): string {
    return gettype($value) . ":" . $value;
}

function native_weak_string(string $value): string {
    return gettype($value) . ":" . $value;
}

function native_weak_bool(bool $value): string {
    return gettype($value) . ":" . ($value ? "1" : "0");
}

echo native_weak_int("42"), "|", native_weak_float("3.5"), "|",
    native_weak_string(17), "|", native_weak_bool("0"), "\n";
echo native_weak_int(true), "|", native_weak_float(8), "|",
    native_weak_string(false), "|", native_weak_bool(2.5), "\n";
echo native_weak_int(7.0), "|", native_weak_float(false), "|",
    native_weak_string(1.25), "|", native_weak_bool(""), "\n";

try {
    native_weak_int("not numeric");
} catch (TypeError $error) {
    echo get_class($error), "|";
}

try {
    native_weak_float(null);
} catch (TypeError $error) {
    echo get_class($error), "\n";
}
