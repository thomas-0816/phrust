<?php
$items = [1, 2, 3, 4];
$assoc = ["a" => 1, "b" => 2, "c" => 3];
$sum = 0;
for ($i = 0; $i < 8; $i++) {
    $sum += count($items);
    $sum += strlen("abcd");
    $sum += is_int($i) ? 1 : 0;
    $sum += count(array_values($assoc));
    $sum += strlen(strtolower("ABC"));
    $sum += str_contains("abcdef", "cd") ? 1 : 0;
    $sum += str_starts_with("abcdef", "ab") ? 1 : 0;
    $sum += str_ends_with("abcdef", "ef") ? 1 : 0;
}
echo "stdlib:", $sum, "\n";
