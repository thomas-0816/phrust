<?php
declare(strict_types=1);

function native_strict_float(float $value): string {
    return gettype($value) . ":" . $value;
}

function native_strict_float_ref(float &$value): string {
    return gettype($value) . ":" . $value;
}

function native_strict_nullable_float(?float $value): string {
    return gettype($value) . ":" . ($value ?? "null");
}

function native_strict_float_return(): float {
    return 10;
}

function &native_strict_float_reference_return(): float {
    static $value = 12;
    return $value;
}

function native_strict_int_return_error(): int {
    return "11";
}

echo native_strict_float(7), "\n";
$reference = 8;
echo native_strict_float_ref($reference), "|", gettype($reference), ":", $reference, "\n";
echo native_strict_nullable_float(null), "|", native_strict_nullable_float(9), "\n";
echo gettype(native_strict_float_return()), ":", native_strict_float_return(), "\n";
$return_reference =& native_strict_float_reference_return();
echo gettype($return_reference), ":", $return_reference, "\n";

try {
    native_strict_float(true);
} catch (TypeError $error) {
    echo get_class($error), "\n";
}

try {
    native_strict_float(null);
} catch (TypeError $error) {
    echo get_class($error), "\n";
}

try {
    native_strict_float("1");
} catch (TypeError $error) {
    echo get_class($error), "\n";
}

try {
    native_strict_int_return_error();
} catch (TypeError $error) {
    echo get_class($error), "\n";
}
