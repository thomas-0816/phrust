<?php

echo "seed\n";

function queryCurrentMemory(bool $real): int
{
    return memory_get_usage($real);
}

function queryPeakMemory(bool $real): int
{
    return memory_get_peak_usage($real);
}

function queryMemoryWithInvalidFlag($real): int
{
    return memory_get_usage($real);
}

$usage = queryCurrentMemory(false);
$realUsage = queryCurrentMemory(true);
$peak = queryPeakMemory(false);
$realPeak = queryPeakMemory(true);

var_dump(is_int($usage));
var_dump(is_int($realUsage));
var_dump(is_int($peak));
var_dump(is_int($realPeak));
var_dump($usage >= 0);
var_dump($realUsage >= 0);
var_dump($peak >= $usage);
var_dump($realPeak >= $realUsage);

try {
    queryMemoryWithInvalidFlag([]);
} catch (TypeError $error) {
    echo get_class($error), "\n";
}
