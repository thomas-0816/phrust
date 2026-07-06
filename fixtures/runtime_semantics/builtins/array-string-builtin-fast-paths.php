<?php
// implode/array_keys/array_merge fast paths must match the generic
// builtins: implode joins with conversions falling back, array_keys keeps
// key types and order, array_merge renumbers int keys and overwrites
// string keys left-to-right.

$strings = ["alpha", "beta", "gamma"];
echo implode("|", $strings), "\n";
echo implode("", $strings), "\n";
echo implode(", ", []), "\n";
// Mixed types use the generic conversion path.
echo implode("-", [1, "two", 3.5, true, null]), "\n";
// NOTE: swapped legacy implode(array, string) raises TypeError in PHP 8;
// the engine's error shape for it is a pre-existing gap pinned elsewhere.

$map = ["b" => 2, 7 => "seven", "a" => 1, 0 => "zero", "10" => "ten"];
var_dump(array_keys($map));
// Filtered forms stay generic.
var_dump(array_keys([1, 2, 1, 3], 1));
var_dump(array_keys(["x" => "1", "y" => 1], 1, true));

$left = ["k" => "old", 0 => "a", 5 => "b"];
$right = ["k" => "new", 0 => "c", "fresh" => true];
var_dump(array_merge($left, $right));
var_dump(array_merge([], $left));
var_dump(array_merge($left));

// Results are independent copies.
$base = ["x" => [1, 2]];
$merged = array_merge($base, ["y" => 3]);
$merged["x"][] = 99;
var_dump(count($base["x"]), count($merged["x"]));

// Keys of a merged result keep insertion order across repeated merges.
$acc = [];
for ($i = 0; $i < 3; $i++) {
    $acc = array_merge($acc, ["k$i" => $i]);
}
echo implode(",", array_keys($acc)), "\n";
