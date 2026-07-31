<?php

$filled = array_fill(-2, 4, 'x');
foreach ($filled as $key => $value) {
    echo $key, '=', $value, ';';
}
echo "\n";

$filledKeys = array_fill_keys(['alpha', 'beta', 'alpha'], 7);
foreach ($filledKeys as $key => $value) {
    echo $key, '=', $value, ';';
}
echo "\n";

$combined = array_combine(['alpha', 'beta'], [1, 2]);
foreach ($combined as $key => $value) {
    echo $key, '=', $value, ';';
}
echo "\n";

$flipped = array_flip(['first' => 'alpha', 'second' => 'beta', 'third' => 'alpha']);
foreach ($flipped as $key => $value) {
    echo $key, '=', $value, ';';
}
echo "\n";

// Numeric-string keys deliberately take the single baseline continuation;
// the continuation must preserve PHP's canonical-key conversion exactly.
$numericKeys = array_fill_keys(['2', '02', '-2', '+2'], 'n');
foreach ($numericKeys as $key => $value) {
    echo $key, '=', $value, ';';
}
echo "\n";

$numericCombined = array_combine(['2', '02', '-2', '+2'], ['a', 'b', 'c', 'd']);
foreach ($numericCombined as $key => $value) {
    echo $key, '=', $value, ';';
}
echo "\n";
