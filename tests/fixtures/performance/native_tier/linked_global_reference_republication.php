<?php
$perf_linked_global_reference = 'first';
require __DIR__ . '/linked_global_reference_republication_target.php';

function perf_linked_global_reference_wrapper() {
    return perf_linked_global_reference_value();
}

echo perf_linked_global_reference_wrapper(), '|';

$replacement = 'second';
$perf_linked_global_reference =& $replacement;
echo perf_linked_global_reference_wrapper(), '|';

unset($GLOBALS['perf_linked_global_reference']);
echo is_null(perf_linked_global_reference_wrapper()) ? 'unset' : 'stale', '|';

$perf_linked_global_reference = 'third';
echo perf_linked_global_reference_wrapper(), "\n";
