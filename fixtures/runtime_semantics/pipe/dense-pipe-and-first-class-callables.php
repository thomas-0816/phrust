<?php

function double(int $n): int
{
    return $n * 2;
}

function suffix(string $s): string
{
    return $s . '!';
}

$fn = double(...);
$sum = 0;
for ($i = 0; $i < 20; $i++) {
    $sum += $fn($i);
}
echo $sum, "\n";

$out = 5 |> double(...) |> double(...);
echo $out, "\n";

echo ('hey' |> suffix(...)), "\n";

$closurePipe = 3 |> (fn (int $n): int => $n + 10) |> double(...);
echo $closurePipe, "\n";

try {
    $missing = missing_target(...);
} catch (Error $error) {
    echo get_class($error), ': ', $error->getMessage(), "\n";
}

$viaString = 'double';
echo (7 |> $viaString), "\n";
