<?php
function native_array_splice_case(
    array $input,
    int $offset,
    ?int $length,
    array $replacement
): array {
    $copy = $input;
    $alias =& $input;
    $removed = array_splice($alias, $offset, $length, $replacement);

    return [
        "mutated" => $input,
        "alias-visible" => $alias === $input,
        "removed" => $removed,
        "copy-unchanged" => $copy,
    ];
}

function native_array_splice_without_replacement(array $input, int $offset, ?int $length): array {
    $copy = $input;
    $removed = array_splice($input, $offset, $length);
    return [$input, $removed, $copy];
}

for ($warm = 0; $warm < 64; $warm++) {
    native_array_splice_case(
        [9 => "nine", "keep" => "K", 12 => "twelve", "tail" => "T"],
        1,
        2,
        ["ignored" => "R1", 8 => "R2"],
    );
    native_array_splice_without_replacement(
        [4 => "a", "middle" => "b", 9 => "c", "tail" => "d"],
        -3,
        -1,
    );
}

var_dump(native_array_splice_case(
    [9 => "nine", "keep" => "K", 12 => "twelve", "tail" => "T"],
    1,
    2,
    ["ignored" => "R1", 8 => "R2"],
));
var_dump(native_array_splice_without_replacement(
    [4 => "a", "middle" => "b", 9 => "c", "tail" => "d"],
    -3,
    -1,
));
