<?php

function native_nullable_int(?int $value): string {
    return gettype($value) . ":" . ($value ?? "null");
}

function native_nullable_float(?float $value): string {
    return gettype($value) . ":" . ($value ?? "null");
}

function native_nullable_string(?string $value): string {
    return gettype($value) . ":" . ($value ?? "null");
}

function native_nullable_bool(?bool $value): string {
    return gettype($value) . ":" . ($value === null ? "null" : ($value ? "1" : "0"));
}

function native_nullable_int_ref(?int &$value): void {
    echo gettype($value), ":", ($value ?? "null"), "|";
}

function native_nullable_string_ref(?string &$value): void {
    echo gettype($value), ":", ($value ?? "null"), "|";
}

echo native_nullable_int(null), "|", native_nullable_int("42"), "\n";
echo native_nullable_float(null), "|", native_nullable_float("3.5"), "\n";
echo native_nullable_string(null), "|", native_nullable_string(17), "\n";
echo native_nullable_bool(null), "|", native_nullable_bool("0"), "\n";

$integer = "7";
$null = null;
$string = 8;
native_nullable_int_ref($integer);
native_nullable_int_ref($null);
native_nullable_string_ref($string);
echo gettype($integer), ":", $integer, "|";
echo gettype($null), ":null|";
echo gettype($string), ":", $string, "\n";
