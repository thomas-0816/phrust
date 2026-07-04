<?php
// Native record-shape lookup region: hot symbol-guarded slot reads.
function perf_record_lookup(array $m, string $k) {
    return $m[$k];
}
$config = ["host" => "db.local", "port" => 5432, "name" => "app"];
$out = "";
for ($i = 0; $i < 8; $i++) {
    $out = $out . perf_record_lookup($config, "host") . ":" . perf_record_lookup($config, "port") . ";";
}
echo strlen($out), "\n";
