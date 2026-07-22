<?php
function native_array_pointer_family(array $array): void {
    var_dump(current($array));
    var_dump(key($array));
    var_dump(next($array));
    var_dump(key($array));
    var_dump(next($array));
    var_dump(next($array));
    var_dump(key($array));
    var_dump(prev($array));
    var_dump(reset($array));
    var_dump(end($array));
}

$source = ["first" => 10, 4 => false, "last" => 30];
native_array_pointer_family($source);
var_dump(current($source));

function native_array_push_pop(array $array): array {
    $length = array_push($array, 10, 20);
    $popped = array_pop($array);
    return [$length, $popped, $array];
}

var_dump(native_array_push_pop([5 => 1]));
