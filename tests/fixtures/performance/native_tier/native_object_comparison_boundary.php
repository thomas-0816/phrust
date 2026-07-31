<?php

final class NativeComparisonBox
{
    public int $value;
    public array $nested;

    public function __construct(int $value, array $nested)
    {
        $this->value = $value;
        $this->nested = $nested;
    }
}

function compare_native_objects(
    NativeComparisonBox $left,
    NativeComparisonBox $same,
    NativeComparisonBox $greater
): array {
    return [
        $left === $same,
        $left == $same,
        $left != $greater,
        $left <=> $greater,
        [$left] === [$same],
        [$left] == [$same],
        [$left] <=> [$greater],
        $left == true,
        $left <=> true,
        [] == null,
        [$left] == true,
        [] == false,
    ];
}

$left = new NativeComparisonBox(1, ['number' => 2, 'nested' => [3]]);
$same = new NativeComparisonBox(1, ['number' => '2', 'nested' => [3]]);
$greater = new NativeComparisonBox(2, ['number' => 2, 'nested' => [3]]);

var_export(compare_native_objects($left, $same, $greater));
echo "\n";
