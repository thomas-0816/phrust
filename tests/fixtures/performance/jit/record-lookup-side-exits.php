<?php
// Native record-lookup side exits: a key miss must resume through the
// interpreter (undefined-key warning + NULL), and a mixed-storage array
// must fail the record-shape guard.
function perf_record_lookup(array $m, string $k) {
    return $m[$k];
}
// Warm past the tiering threshold so the exits below hit native code.
$record = ["present" => 1];
$sink = 0;
for ($i = 0; $i < 24; $i++) {
    $sink = $sink + perf_record_lookup($record, "present");
}
echo $sink, "\n";
var_dump(perf_record_lookup($record, "absent"));

$mixed = [7, 8, 9];
$mixed["late"] = 10;
echo perf_record_lookup($mixed, "late"), "\n";
