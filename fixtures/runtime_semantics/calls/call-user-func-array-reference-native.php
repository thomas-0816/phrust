<?php
// runtime-semantics: category=calls expect=pass

function native_bump_pair(int &$first, int &$second): int
{
    $first += 1;
    $second += 2;
    return $first + $second;
}

function native_bump_variadic(int &...$values): int
{
    $values[0] += 3;
    $values[1] += 4;
    return $values[0] + $values[1];
}

function run_native_reference_unpack(): array
{
    $first = 10;
    $second = 20;
    $arguments = [&$first, &$second];

    $fixed = call_user_func_array('native_bump_pair', $arguments);
    $variadic = call_user_func_array('native_bump_variadic', $arguments);

    $assigned = [];
    $assigned['first'] = $first;
    $assigned[] = $second;

    $nested = [];
    $nested['values'][] = $first;

    $spread = [...$arguments];

    return [
        $fixed,
        $variadic,
        $first,
        $second,
        $arguments[0],
        $arguments[1],
        $assigned,
        $nested,
        $spread,
    ];
}

$result = null;
for ($iteration = 0; $iteration < 32; $iteration++) {
    $result = run_native_reference_unpack();
}
var_dump($result);
