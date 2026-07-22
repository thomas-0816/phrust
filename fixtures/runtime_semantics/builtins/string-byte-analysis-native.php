<?php
function native_string_byte_analysis(string $value, string $needle): array {
    return [addslashes($value), substr_count($value, $needle)];
}

var_dump(native_string_byte_analysis("a\0'b\"c\\d-a\0'b\"c\\d", "a"));
var_dump(native_string_byte_analysis("aaaa", "aa"));
var_dump(native_string_byte_analysis("binary\0binary\0", "\0"));
