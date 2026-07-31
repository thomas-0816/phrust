<?php

function baseline_closure_reference_valid(): int
{
    $calls = 0;
    try {
        array_map(
            function (int $value) use (&$calls): int {
                ++$calls;
                return $value * 2;
            },
            [1, 2, 3],
        );
    } catch (Throwable) {
    }

    return $calls;
}

function baseline_closure_reference_type_error(): int
{
    $calls = 0;
    try {
        array_map(
            function (int $value) use (&$calls): int {
                ++$calls;
                return $value;
            },
            [1, []],
        );
    } catch (TypeError) {
    }

    return $calls;
}

function baseline_closure_existing_alias(): string
{
    $value = 1;
    $alias =& $value;
    try {
        array_map(
            function (int $increment) use (&$alias): int {
                $alias += $increment;
                return $alias;
            },
            [2, 4],
        );
    } catch (Throwable) {
    }

    return $value . ':' . $alias;
}

function baseline_closure_reference_escape(): Closure
{
    $counter = 0;

    return function () use (&$counter): int {
        return ++$counter;
    };
}

echo baseline_closure_reference_valid(), "\n";
echo baseline_closure_reference_type_error(), "\n";
echo baseline_closure_existing_alias(), "\n";
$escaped = baseline_closure_reference_escape();
echo $escaped(), ':', $escaped(), "\n";
