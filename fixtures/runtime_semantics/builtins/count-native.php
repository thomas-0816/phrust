<?php
// runtime-semantics: category=builtins expect=pass php_ref_required=1
// One completed vertical boundary: fixed count/sizeof native targets plus
// their single pre-effect baseline continuation for non-direct shapes.

$inner = [2, 3, 4];
$outer = [1, $inner, 5];
$alias = &$outer;

echo count($alias), "\n";
echo count($alias, COUNT_NORMAL), "\n";
echo count($alias, COUNT_RECURSIVE), "\n";
echo sizeof($alias, COUNT_RECURSIVE), "\n";

final class NativeSized implements Countable {
    public function count(): int {
        return 17;
    }
}

echo count(new NativeSized()), "\n";

function invalid_count_mode(array &$value): string {
    try {
        count($value, 2);
    } catch (ValueError $error) {
        return get_class($error);
    }
    return "missing";
}

echo invalid_count_mode($alias), "\n";
