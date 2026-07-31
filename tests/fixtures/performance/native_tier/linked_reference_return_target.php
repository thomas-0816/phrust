<?php

$perf_linked_reference_return_storage = null;

function &perf_linked_reference_return_target(int $initial)
{
    global $perf_linked_reference_return_storage;
    if ($perf_linked_reference_return_storage === null) {
        $perf_linked_reference_return_storage = $initial;
    }
    return $perf_linked_reference_return_storage;
}
