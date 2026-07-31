<?php
function perf_late_linked_signature(int $value, string $suffix = 'default') {
    return $value . ':' . $suffix;
}
