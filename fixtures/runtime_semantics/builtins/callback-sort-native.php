<?php
function native_callback_sort_compare($left, $right): int {
    return $left <=> $right;
}

function native_callback_usort_case(): array {
    $values = [9 => 30, "keep" => 10, 12 => 20, "tail" => 20];
    $copy = $values;
    $alias =& $values;
    $result = usort($alias, "native_callback_sort_compare");
    return [$result, $values, $alias === $values, $copy];
}

function native_callback_uasort_case(): array {
    $values = [9 => 30, "keep" => 10, 12 => 20, "tail" => 20];
    $copy = $values;
    $result = uasort($values, "native_callback_sort_compare");
    return [$result, $values, $copy];
}

function native_callback_uksort_case(): array {
    $values = ["zulu" => 1, "alpha" => 2, "mike" => 3, "bravo" => 4];
    $copy = $values;
    $result = uksort($values, "native_callback_sort_compare");
    return [$result, $values, $copy];
}

function native_dynamic_usort_case(callable $callback): array {
    $values = [9 => 30, "keep" => 10, 12 => 20, "tail" => 20];
    $copy = $values;
    $result = usort($values, $callback);
    return [$result, $values, $copy];
}

function native_dynamic_uasort_case(callable $callback): array {
    $values = [9 => 30, "keep" => 10, 12 => 20, "tail" => 20];
    $copy = $values;
    $result = uasort($values, $callback);
    return [$result, $values, $copy];
}

function native_dynamic_uksort_case(callable $callback): array {
    $values = ["zulu" => 1, "alpha" => 2, "mike" => 3, "bravo" => 4];
    $copy = $values;
    $result = uksort($values, $callback);
    return [$result, $values, $copy];
}

final class NativeDynamicComparator {
    public function compare(int|string $left, int|string $right): int {
        return $left <=> $right;
    }
}

$dynamicComparator = new NativeDynamicComparator();
$dynamicClosure = static fn (int $left, int $right): int => $left <=> $right;

for ($warm = 0; $warm < 64; $warm++) {
    native_callback_usort_case();
    native_callback_uasort_case();
    native_callback_uksort_case();
    native_dynamic_usort_case("native_callback_sort_compare");
    native_dynamic_uasort_case($dynamicClosure);
    native_dynamic_uksort_case([$dynamicComparator, "compare"]);
}

var_dump(native_callback_usort_case());
var_dump(native_callback_uasort_case());
var_dump(native_callback_uksort_case());
var_dump(native_dynamic_usort_case("native_callback_sort_compare"));
var_dump(native_dynamic_uasort_case($dynamicClosure));
var_dump(native_dynamic_uksort_case([$dynamicComparator, "compare"]));
