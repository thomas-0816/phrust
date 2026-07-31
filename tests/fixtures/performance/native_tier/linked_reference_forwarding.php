<?php

require __DIR__ . '/linked_reference_forwarding_target.php';

function perf_linked_reference_forwarding_wrapper(&$value): int
{
    return perf_linked_reference_forwarding_target($value);
}

function perf_linked_local_reference_promotion(int $value): int
{
    perf_linked_reference_forwarding_target($value);
    return $value;
}

function perf_linked_string_reference_promotion(): string
{
    $value = 'native';
    perf_linked_reference_append_string($value);
    return $value;
}

function perf_linked_array_reference_promotion(): string
{
    $value = [1, 2];
    perf_linked_reference_append_array($value);
    return implode(',', $value);
}

$value = 7;
echo perf_linked_reference_forwarding_wrapper($value), '|', $value, '|';
echo perf_linked_local_reference_promotion(11), '|';
echo perf_linked_string_reference_promotion(), '|';
echo perf_linked_array_reference_promotion(), "\n";
