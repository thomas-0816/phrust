<?php

$sum = 0;
for ($i = 1; $i <= 1000; $i++) {
    $sum += abs(-$i);
    $sum += ord('A');
    $sum += strpos('abc', 'b');
    $sum += str_contains('abc', 'b');
    $sum += str_starts_with('abc', 'a');
    $sum += str_ends_with('abc', 'c');
}

echo "compact-builtins:", $sum, "\n";
