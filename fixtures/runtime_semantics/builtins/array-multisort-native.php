<?php
function native_array_multisort_case(): array {
    $primary = [9 => 2, "left" => 1, 12 => 2, "tail" => 1];
    $secondary = [4 => "b", "s-left" => "z", 8 => "a", "s-tail" => "a"];
    $tertiary = [7 => "item10", "t-left" => "Item2", 11 => "item1", "t-tail" => "item12"];

    $result = array_multisort(
        $primary,
        SORT_ASC,
        SORT_NUMERIC,
        $secondary,
        SORT_DESC,
        SORT_STRING,
        $tertiary,
        SORT_ASC,
        SORT_NATURAL | SORT_FLAG_CASE,
    );

    return [$result, $primary, $secondary, $tertiary];
}

function native_array_multisort_cow_case(): array {
    $primary = [5 => 3, "keep" => 1, 9 => 2];
    $copy = $primary;
    $secondary = ["first" => "c", 7 => "a", "last" => "b"];
    array_multisort($primary, SORT_ASC, SORT_REGULAR, $secondary, SORT_DESC, SORT_STRING);
    return [$primary, $secondary, $copy];
}

function native_array_multisort_single_case(): array {
    $values = [8 => 3, "keep" => 1, 12 => 2];
    $result = array_multisort($values);
    return [$result, $values];
}

for ($warm = 0; $warm < 64; $warm++) {
    native_array_multisort_case();
    native_array_multisort_cow_case();
    native_array_multisort_single_case();
}

var_dump(native_array_multisort_case());
var_dump(native_array_multisort_cow_case());
var_dump(native_array_multisort_single_case());
