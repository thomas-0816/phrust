<?php

function ack(int $m, int $n): int {
    if ($m === 0) {
        return $n + 1;
    }
    if ($n === 0) {
        return ack($m - 1, 1);
    }
    return ack($m - 1, ack($m, $n - 1));
}

$m = (int)($argv[1] ?? 3);
$n = (int)($argv[2] ?? 8);

$start = hrtime(true);
$result = ack($m, $n);
$elapsed = hrtime(true) - $start;

printf("ack(%d,%d) = %d\n", $m, $n, $result);
printf("time: %.6f s\n", $elapsed / 1e9);
