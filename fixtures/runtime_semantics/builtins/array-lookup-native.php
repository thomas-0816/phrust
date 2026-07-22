<?php
function native_array_lookup(mixed $needle, array $haystack): array {
    return [
        in_array($needle, $haystack),
        array_search($needle, $haystack),
    ];
}

var_dump(native_array_lookup("needle", ["alpha", "needle", "omega"]));
var_dump(native_array_lookup("missing", ["alpha", "needle", "omega"]));
var_dump(native_array_lookup(2, [1, 2, 3]));

// Numeric strings and cross-kind loose comparisons deliberately use the
// function's single exact baseline continuation.
var_dump(native_array_lookup("01", ["1"]));
var_dump(native_array_lookup(0, [false]));
