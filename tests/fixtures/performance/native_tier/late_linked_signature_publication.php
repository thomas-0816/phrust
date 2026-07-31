<?php
function perf_late_linked_signature_wrapper($value) {
    return perf_late_linked_signature(suffix: 'published', value: $value);
}

require __DIR__ . '/late_linked_signature_publication_target.php';

echo perf_late_linked_signature_wrapper(42), "\n";
