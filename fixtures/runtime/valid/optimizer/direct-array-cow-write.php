<?php
// runtime-fixture: kind=valid expected_stdout="start|copy|scalar|nested|append|1|2|3|4|1|2\nref|11|2\nundefined|20|21\n"

function optimizer_direct_array_cow_write(): void
{
    $original = ['x' => 1, 'nested' => ['v' => 3]];
    echo 'start|';
    $copy = $original;
    echo 'copy|';
    $copy['x'] = 2;
    echo 'scalar|';
    $copy['nested']['v'] = 4;
    echo 'nested|';
    $copy['nested'][] = 5;
    echo 'append|';
    echo $original['x'], '|', $copy['x'], '|';
    echo $original['nested']['v'], '|', $copy['nested']['v'], '|';
    echo count($original['nested']), '|', count($copy['nested']), "\n";

    echo 'ref|';
    $value = ['x' => 10];
    $reference =& $value;
    $reference['x'] = 11;
    $reference[] = 12;
    echo $value['x'], '|', count($value), "\n";

    echo 'undefined|';
    $undefined['x'] = 20;
    $appended[] = 21;
    echo $undefined['x'], '|', $appended[0], "\n";
}

optimizer_direct_array_cow_write();
