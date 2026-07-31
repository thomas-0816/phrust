<?php
function native_array_sum_family(array $values): int|float {
    return array_sum($values);
}

var_dump(native_array_sum_family([]));
var_dump(native_array_sum_family([1, 2, 3, 4]));
var_dump(native_array_sum_family([1, 2.5, "3", true, null]));
var_dump(native_array_sum_family([PHP_INT_MAX, 1]) === (float) PHP_INT_MAX + 1.0);
