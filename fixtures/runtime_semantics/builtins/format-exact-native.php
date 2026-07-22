<?php

function run_exact_format(): array
{
    $formatted = sprintf('%s:%04d:%.1f', 'item', 7, 2.5);
    $vectorFormatted = vsprintf('%2$s/%1$03d', [5, 'value']);
    $printed = printf('[%s=%d]', 'count', 3);
    $vectorPrinted = vprintf('<%s:%02d>\n', ['id', 4]);

    return [$formatted, $vectorFormatted, $printed, $vectorPrinted];
}

$result = null;
for ($iteration = 0; $iteration < 32; $iteration++) {
    $result = run_exact_format();
}
var_dump($result);
