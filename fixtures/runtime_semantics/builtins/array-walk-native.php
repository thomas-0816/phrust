<?php
function native_array_walk_add(&$value, $key, $userdata): void {
    $keyWeight = is_int($key) ? $key : strlen($key);
    $value = $value + $userdata + $keyWeight;
}

function native_array_walk_label(&$value, $key): void {
    $value = $key . ":" . $value;
}

function native_array_walk_userdata_case(): array {
    $values = [2 => 10, "abc" => 20, 7 => 30];
    $copy = $values;
    $result = array_walk($values, "native_array_walk_add", 3);
    return [$result, $values, $copy];
}

function native_array_walk_without_userdata_case(): array {
    $values = [4 => "a", "name" => "b", 9 => "c"];
    $copy = $values;
    $result = array_walk($values, "native_array_walk_label");
    return [$result, $values, $copy];
}

for ($warm = 0; $warm < 64; $warm++) {
    native_array_walk_userdata_case();
    native_array_walk_without_userdata_case();
}

var_dump(native_array_walk_userdata_case());
var_dump(native_array_walk_without_userdata_case());
