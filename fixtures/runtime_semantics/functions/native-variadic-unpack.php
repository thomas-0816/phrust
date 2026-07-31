<?php
function native_variadic_unpack($first, string ...$rest)
{
    $before = func_num_args()
        . ":" . func_get_arg(0)
        . ":" . func_get_arg(2);
    $rest[0] = "changed";
    return $before
        . ":" . implode(",", func_get_args())
        . ":" . implode(",", $rest);
}

function native_fixed_unpack($first)
{
    return func_num_args()
        . ":" . func_get_arg(2)
        . ":" . implode(",", func_get_args());
}

$values = ["A", "B", "C", "D"];
echo native_variadic_unpack(...$values), "\n";
echo native_fixed_unpack(...$values), "\n";
echo implode(",", $values), "\n";
