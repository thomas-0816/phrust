<?php

function native_compact_projection(): void
{
    $scalar = 7;
    $array = ['nested' => [2, 3]];
    $alias =& $array;
    $names = ['scalar', ['array', 'alias']];

    $projected = compact($names, 'scalar');
    var_dump(array_keys($projected) === ['scalar', 'array', 'alias']);
    var_dump($projected['scalar'] === 7);
    var_dump($projected['array'] === ['nested' => [2, 3]]);
    var_dump($projected['alias'] === ['nested' => [2, 3]]);

    $array['nested'][] = 5;
    var_dump($projected['array'] === ['nested' => [2, 3]]);
    var_dump($projected['alias'] === ['nested' => [2, 3]]);
    var_dump($array === ['nested' => [2, 3, 5]]);
}

native_compact_projection();

$topLevel = 'visible';
var_dump(compact('topLevel') === ['topLevel' => 'visible']);
var_dump(@compact('missing') === []);
