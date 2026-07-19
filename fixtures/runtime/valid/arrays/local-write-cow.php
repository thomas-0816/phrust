<?php

function exercise_array_writes(): void
{
    $original = ['x' => 1, 'nested' => ['v' => 3]];
    $copy = $original;
    $copy['x'] = 2;
    $copy['nested']['v'] = 4;
    $copy['nested'][] = 5;
    echo $original['x'], '|', $copy['x'], '|';
    echo $original['nested']['v'], '|', $copy['nested']['v'], '|';
    echo count($original['nested']), '|', count($copy['nested']), "\n";

    $value = ['x' => 10];
    $reference =& $value;
    $reference['x'] = 11;
    $reference[] = 12;
    echo $value['x'], '|', count($value), "\n";

    $undefined['x'] = 20;
    $appended[] = 21;
    echo $undefined['x'], '|', $appended[0], "\n";
}

exercise_array_writes();
