<?php
require __DIR__ . '/cross_unit_handle_transfer_target.php';

function perf_late_linked_wrapper() {
    return perf_cross_unit_literal() . '|' . perf_cross_unit_argument('caller-linked');
}

echo perf_late_linked_wrapper(), "\n";
