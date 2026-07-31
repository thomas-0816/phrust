<?php

require __DIR__ . '/linked_reference_return_target.php';

function perf_linked_reference_return_wrapper(int $delta): int
{
    $alias =& perf_linked_reference_return_target(5);
    $alias += $delta;
    return $alias;
}

echo perf_linked_reference_return_wrapper(4), '|';
$same_alias =& perf_linked_reference_return_target(100);
echo $same_alias, "\n";
