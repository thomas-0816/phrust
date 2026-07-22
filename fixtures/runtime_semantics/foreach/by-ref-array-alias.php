<?php
$items = [1, 2];
$alias =& $items;
foreach ($alias as &$value) {
    $value += 10;
}
unset($value);
echo implode(',', $items), "\n";
echo implode(',', $alias), "\n";
