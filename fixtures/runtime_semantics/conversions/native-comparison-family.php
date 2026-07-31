<?php
// runtime-semantics: category=conversions expect=pass
// Exact native comparison family over the authoritative native value graph.

function native_comparison_family(mixed $left, mixed $right): void
{
    var_dump($left == $right);
    var_dump($left != $right);
    var_dump($left === $right);
    var_dump($left !== $right);
    var_dump($left < $right);
    var_dump($left <= $right);
    var_dump($left > $right);
    var_dump($left >= $right);
    var_dump($left <=> $right);
}

echo "int-string\n";
native_comparison_family(10, "10");
echo "numeric-strings\n";
native_comparison_family("10", "2");
echo "nan\n";
native_comparison_family(NAN, 0.0);

$ordered = [0 => "left", "x" => 1];
$reordered = ["x" => 1, 0 => "left"];
echo "arrays\n";
native_comparison_family($ordered, $reordered);

class NativeComparisonBox
{
    public int $value = 1;
}

$left = new NativeComparisonBox();
$right = new NativeComparisonBox();
echo "objects\n";
native_comparison_family($left, $right);

$value = "12";
$reference =& $value;
echo "reference\n";
native_comparison_family($reference, 12);
