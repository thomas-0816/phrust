<?php
// A reference-holding slot fails the read-only slot guard: the compiled
// region side-exits on every invocation and the interpreter fallback
// observes the live reference.
function perf_record_lookup(array $m, string $k) {
    return $m[$k];
}
$cell = 41;
$m = ["cell" => &$cell];
$sink = 0;
for ($i = 0; $i < 12; $i++) {
    $sink = $sink + perf_record_lookup($m, "cell");
}
$cell = 42;
echo $sink, "|", perf_record_lookup($m, "cell"), "\n";
