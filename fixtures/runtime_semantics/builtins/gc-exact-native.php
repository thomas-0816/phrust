<?php

function exerciseGcState(): array
{
    $initial = gc_enabled();
    gc_disable();
    $disabled = !gc_enabled();
    gc_enable();
    $enabled = gc_enabled();
    $collected = gc_collect_cycles();
    $freed = gc_mem_caches();
    $status = gc_status();

    return [
        $initial,
        $disabled,
        $enabled,
        is_int($collected) && $collected >= 0,
        is_int($freed) && $freed >= 0,
        is_array($status),
        isset($status['running']) && is_bool($status['running']),
        isset($status['runs']) && is_int($status['runs']),
        isset($status['collected']) && is_int($status['collected']),
        isset($status['threshold']) && is_int($status['threshold']),
        isset($status['roots']) && is_int($status['roots']),
    ];
}

foreach (exerciseGcState() as $result) {
    var_dump($result);
}
