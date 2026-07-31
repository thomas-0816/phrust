<?php
function perf_cross_unit_identity($object, $text, $number) {
    return $object;
}

function perf_cross_unit_literal() {
    return 'linked';
}

function perf_cross_unit_argument(string $value): string {
    return $value;
}

function perf_cross_unit_sum(int $carry, int $value): int {
    return $carry + $value;
}

function perf_cross_unit_typed_count(int $value): int {
    global $perf_cross_unit_typed_calls;
    ++$perf_cross_unit_typed_calls;
    return $value;
}

function perf_cross_unit_closure_factory(int $offset): Closure {
    return fn (int $value): int => $value + $offset;
}
