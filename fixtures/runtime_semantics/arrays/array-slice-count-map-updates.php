<?php
// runtime-semantics: category=arrays expect=pass php_ref_required=1
// array_slice fast path (packed, negative offsets/lengths, preserve_keys,
// mixed/record arrays), count shapes, and grouped map-update loops through
// by-ref caches with COW isolation.

// Packed slices: offsets, lengths, out-of-range, negative combinations.
$packed = [10, 20, 30, 40, 50, 60, 70];
echo implode(",", array_slice($packed, 3, 8)), "\n";
echo implode(",", array_slice($packed, 0)), "|", implode(",", array_slice($packed, 7)), "\n";
echo implode(",", array_slice($packed, -3)), "|", implode(",", array_slice($packed, -9, 2)), "\n";
echo implode(",", array_slice($packed, 2, -2)), "|", implode(",", array_slice($packed, 2, -9)), "\n";
echo implode(",", array_slice($packed, 1, 0)), "|", implode(",", array_slice($packed, -1, null)), "\n";

// preserve_keys and non-packed shapes keep the generic path.
print_r(array_slice($packed, 4, 2, true));
$record = ["a" => 1, "b" => 2, "c" => 3, "d" => 4];
print_r(array_slice($record, 1, 2));
$mixed = [0 => "x", "k" => "y", 5 => "z"];
print_r(array_slice($mixed, 1));
print_r(array_slice($mixed, 0, null, true));

// Slice must not mutate or share storage with the source.
$source = [1, 2, 3, 4];
$sliced = array_slice($source, 1, 2);
$sliced[0] = 99;
echo implode(",", $source), "|", implode(",", $sliced), "\n";

// count over shapes, references, mode, Countable.
$byref = [1, 2, 3];
$alias = &$byref;
echo count($packed), "|", count($record), "|", count($alias), "\n";
echo count([[1, 2], [3]], COUNT_RECURSIVE), "\n";
echo sizeof([[1, 2], [3]], COUNT_RECURSIVE), "\n";
class Sized implements Countable {
    public function count(): int { return 42; }
}
echo count(new Sized()), "\n";
try {
    count([1], 2);
} catch (ValueError $error) {
    echo $error->getMessage(), "\n";
}
$recursive = [];
$recursive[] = &$recursive;
echo count($recursive, COUNT_RECURSIVE), "\n";

// Grouped map updates through a by-ref cache (session-policy shape).
function tally(&$cache, $key) {
    if (isset($cache[$key])) {
        $cache[$key] = $cache[$key] + 1;
        return $cache[$key];
    }
    $cache[$key] = 1;
    return 1;
}
$cache = [];
$hits = 0;
foreach (["a:1", "b:2", "a:1", "c:3", "a:1", "b:2"] as $key) {
    $hits += tally($cache, $key);
}
echo $hits, "|";
foreach ($cache as $k => $v) {
    echo "$k=$v,";
}
echo "\n";

// COW isolation: a shared handle must not observe in-place writes.
$original = ["x" => 1, "y" => 2];
$snapshot = $original;
$original["x"] = 100;
$original["z"] = 3;
echo $snapshot["x"], "|", isset($snapshot["z"]) ? "leaked" : "isolated", "|", $original["x"], "\n";

// Iteration snapshot: mutating the array inside foreach must not affect
// the current iteration.
$iter = ["p" => 1, "q" => 2];
$seen = "";
foreach ($iter as $k => $v) {
    $iter[$k] = $v * 10;
    $iter["new_$k"] = $v;
    $seen .= "$k=$v,";
}
echo $seen, "|", count($iter), "\n";

// Nested vivification through dim writes, and reference elements.
$deep = [];
$deep["a"]["b"]["c"] = 7;
echo $deep["a"]["b"]["c"], "\n";
$cell = 5;
$holder = ["ref" => &$cell];
$holder["ref"] = 9;
echo $cell, "|", $holder["ref"], "\n";

// Appends stay packed and cheap.
$grow = [];
for ($i = 0; $i < 5; $i++) {
    $grow[] = $i * $i;
}
echo implode(",", $grow), "\n";

// Backtrace args for by-ref parameters observe later writes (the trace
// holds the live reference, not a call-time snapshot).
function mutate_and_throw(&$list) {
    $list[] = "post";
    throw new RuntimeException("trace");
}
try {
    $trace_arr = ["pre"];
    mutate_and_throw($trace_arr);
} catch (RuntimeException $e) {
    echo implode(",", $e->getTrace()[0]["args"][0]), "\n";
}

// (Scalar-as-array dim writes raise an uncatchable engine fatal instead
// of the reference's catchable Error - separate pre-existing known gap.)
