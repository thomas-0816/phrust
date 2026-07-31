<?php

function perf_linked_reference_forwarding_target(&$value): int
{
    ++$value;
    return $value;
}

function perf_linked_reference_append_string(&$value): void
{
    $value .= '!';
}

function perf_linked_reference_append_array(&$value): void
{
    $value[] = 3;
}
