<?php
function native_reindexing_sort_family(): array {
    $ascending = ["left" => 3, 9 => 1, "right" => 2];
    $ascendingResult = sort($ascending);

    $descending = ["first" => "10", "second" => "2", "third" => "1"];
    $descendingResult = rsort($descending, SORT_NUMERIC);

    return [
        $ascendingResult,
        $ascending,
        $descendingResult,
        $descending,
    ];
}

var_dump(native_reindexing_sort_family());
