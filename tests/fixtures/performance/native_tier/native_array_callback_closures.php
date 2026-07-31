<?php

function native_array_callback_closures(array $input)
{
    $offset = 3;
    $mapped = array_map(fn(int $value): int => $value + $offset, $input);

    $threshold = 4;
    $filtered = array_filter($mapped, fn(int $value): bool => $value > $threshold);

    $factor = 2;
    return array_reduce(
        $filtered,
        fn(int $carry, int $value): int => $carry + ($value * $factor),
        1,
    );
}

echo native_array_callback_closures([1, 2, 3]), "\n";

final class NativeArrayCallbackTransformer
{
    public static function twice(int $value): int
    {
        return $value * 2;
    }
}

function native_static_array_callback()
{
    return implode(
        ",",
        array_map("NativeArrayCallbackTransformer::twice", [2, 4]),
    );
}

echo native_static_array_callback(), "\n";

$referenced = 7;
$reference_input = [&$referenced];
$reference_map = array_map(fn($value) => $value, $reference_input);
$reference_map[0] = 11;
echo $referenced, ",", $reference_map[0], "\n";

$reference_filter = array_filter($reference_input, fn($value) => true);
$reference_filter[0] = 13;
echo $referenced, ",", $reference_filter[0], "\n";

function native_reference_capture_callback()
{
    $calls = 0;
    $mapped = array_map(
        function (int $value) use (&$calls): int {
            ++$calls;
            return $value;
        },
        [1, 2, 3],
    );
    return $calls . ":" . implode(",", $mapped);
}

echo native_reference_capture_callback(), "\n";

function native_adder_factory(int $offset): Closure
{
    return fn (int $value): int => $value + $offset;
}

function native_returned_closure_callback(): string
{
    $adder = native_adder_factory(6);
    return implode(",", array_map($adder, [1, 3]));
}

echo native_returned_closure_callback(), "\n";

function native_closure_call_user_func(): int
{
    $offset = 5;
    $callback = fn(int $value): int => $value + $offset;
    return call_user_func($callback, 2);
}

function native_closure_call_user_func_array(): int
{
    $offset = 5;
    $callback = fn(int $value): int => $value + $offset;
    return call_user_func_array($callback, [4]);
}

echo native_closure_call_user_func(), ',', native_closure_call_user_func_array(), "\n";

$native_typed_callback_calls = 0;

function native_typed_callback_mismatch_direct()
{
    array_map(
        function (int $value): int {
            global $native_typed_callback_calls;
            ++$native_typed_callback_calls;
            return $value;
        },
        [1, "not-an-int", 3],
    );
}

try {
    native_typed_callback_mismatch_direct();
} catch (TypeError $error) {
}
echo $native_typed_callback_calls, "\n";

function native_optional_array_callback(int $value, int $offset = 10): int
{
    return $value + $offset;
}

echo implode(",", array_map("native_optional_array_callback", [1, 2])), "\n";

function native_variadic_map_callback(int $head, int ...$tail): string
{
    return $head . ":" . array_sum($tail);
}

echo implode(
    ",",
    array_map(
        "native_variadic_map_callback",
        [1, 2],
        [10, 20],
        [100, 200],
    ),
), "\n";

function native_variadic_filter_callback(int $value, string ...$keys): bool
{
    return $value === 2 && $keys === ["b"];
}

echo implode(
    ",",
    array_keys(
        array_filter(
            ["a" => 1, "b" => 2],
            "native_variadic_filter_callback",
            ARRAY_FILTER_USE_BOTH,
        ),
    ),
), "\n";

function native_variadic_reduce_callback(int $carry, int ...$values): int
{
    return $carry + array_sum($values);
}

echo array_reduce([1, 2, 3], "native_variadic_reduce_callback", 0), "\n";

function &native_map_reference_callback(int $value)
{
    static $slot;
    $slot = $value * 10;
    return $slot;
}

$native_reference_result = array_map("native_map_reference_callback", [1, 2]);
$native_reference_result[0] = 77;
echo implode(",", $native_reference_result), "\n";

function &native_filter_reference_callback(int $value)
{
    static $keep;
    $keep = $value % 2;
    return $keep;
}

echo implode(
    ",",
    array_keys(array_filter([1, 2, 3], "native_filter_reference_callback")),
), "\n";

function &native_reduce_reference_callback(int $carry, int $value)
{
    static $sum;
    $sum = $carry + $value;
    return $sum;
}

$native_reduce_reference = array_reduce(
    [1, 2, 3],
    "native_reduce_reference_callback",
    0,
);
$native_reduce_alias =& native_reduce_reference_callback(0, 20);
$native_reduce_alias = 99;
echo $native_reduce_reference, ",", $native_reduce_alias, "\n";

$native_by_reference_warnings = 0;
set_error_handler(
    function () use (&$native_by_reference_warnings): bool {
        ++$native_by_reference_warnings;
        return true;
    },
);
$native_by_reference_input = [1, 2];
$native_by_reference_result = array_map(
    function (&$value): int {
        return ++$value;
    },
    $native_by_reference_input,
);
restore_error_handler();
echo
    $native_by_reference_warnings,
    ":",
    implode(",", $native_by_reference_result),
    ":",
    implode(",", $native_by_reference_input),
    "\n";

function native_array_predicate_positive(int $value, $key): bool
{
    return $value > 0;
}

function native_array_predicate_selected(int $value, $key): bool
{
    return $value > 2 && $key !== "skip";
}

$native_predicate_input = [
    "a" => 1,
    "b" => 3,
    "skip" => 5,
    "d" => 4,
];
echo (int) array_all(
    $native_predicate_input,
    "native_array_predicate_positive",
), "\n";
echo (int) array_any(
    $native_predicate_input,
    "native_array_predicate_selected",
), "\n";
echo array_find(
    $native_predicate_input,
    "native_array_predicate_selected",
), "\n";
echo array_find_key(
    $native_predicate_input,
    "native_array_predicate_selected",
), "\n";

$native_predicate_limit = 3;
echo array_find(
    $native_predicate_input,
    fn(int $value, $key): bool => $value > $native_predicate_limit,
), "\n";
echo
    (int) array_all([], "native_array_predicate_positive"),
    ":",
    (int) array_any([], "native_array_predicate_positive"),
    ":",
    array_find([], "native_array_predicate_positive") === null ? "null" : "value",
    ":",
    array_find_key([], "native_array_predicate_positive") === null ? "null" : "key",
    "\n";
