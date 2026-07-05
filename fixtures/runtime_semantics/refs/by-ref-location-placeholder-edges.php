<?php

function grow(array &$rows, int ...$extra): void {
    foreach ($extra as $value) {
        $rows[] = $value;
    }
    $rows[] = 'grown';
}

if (true) {
    function conditional_bump(int &$n): void {
        $n += 100;
    }
}

function observes(array &$items): array {
    $items[] = 'seen';
    return [func_num_args(), count($items)];
}

$rows = ['seed'];
grow($rows, 1, 2);
var_dump($rows);

$n = 1;
conditional_bump($n);
var_dump($n);

$items = ['x'];
var_dump(observes($items));
var_dump($items);

$packed = [['p'], ['q']];
function first_only(array &$target): void {
    $target[] = 'first';
}
first_only(...[&$packed[0]]);
var_dump($packed[0]);

function needs_two(array &$rows, int $must): void {
    $rows[] = $must;
}
try {
    needs_two($rows);
} catch (ArgumentCountError $error) {
    echo get_class($error), ': ', $error->getMessage(), "\n";
}
var_dump(count($rows));
