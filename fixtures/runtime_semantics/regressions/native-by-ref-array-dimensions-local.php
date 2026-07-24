<?php

function increment_local_array_dimension(&$value) {
    $value++;
}

function exercise_local_array_dimensions() {
    $nested = [[1]];
    increment_local_array_dimension($nested[0][0]);

    $missing = [];
    increment_local_array_dimension($missing['created']);

    $aliased = 1;
    $references = [&$aliased];
    increment_local_array_dimension($references[0]);

    $copy_on_write = [1];
    $snapshot = $copy_on_write;
    increment_local_array_dimension($copy_on_write[0]);

    $strings = ['key' => 1];
    increment_local_array_dimension($strings['key']);

    return $nested[0][0]
        . ':' . $missing['created']
        . ':' . $aliased . ':' . $references[0]
        . ':' . $copy_on_write[0] . ':' . $snapshot[0]
        . ':' . $strings['key'];
}

echo exercise_local_array_dimensions();
