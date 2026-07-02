<?php
function perf_server_call(int $value): int {
    return $value * 3 + 1;
}
$sum = 0;
for ($i = 0; $i < 100; $i++) {
    $sum += perf_server_call($i);
}
echo "calls:", $sum, "\n";
