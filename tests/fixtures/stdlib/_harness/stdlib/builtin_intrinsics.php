<?php
// stdlib-diff: id=STDLIB_BUILTIN_INTRINSICS area=stdlib expect=pass
function show_error($label, $fn) {
    try {
        $fn();
        echo $label, ":no-error\n";
    } catch (Throwable $e) {
        echo $label, ":", get_class($e), "\n";
    }
}

$binary = "ab\0cd";
echo "contains-binary:", str_contains($binary, "\0c") ? "yes" : "no", "\n";
echo "contains-empty:", str_contains("", "") ? "yes" : "no", "\n";
echo "contains-large:", str_contains(str_repeat("a", 4096) . "z", "az") ? "yes" : "no", "\n";
echo "contains-named:", str_contains(needle: "bc", haystack: "abcd") ? "yes" : "no", "\n";
$contains_ref = "abc";
$contains_alias =& $contains_ref;
echo "contains-ref:", str_contains($contains_alias, "b") ? "yes" : "no", "\n";
show_error("contains-arity", fn() => str_contains("abc"));
show_error("contains-type", fn() => str_contains([], "a"));

echo "starts-binary:", str_starts_with("\0abc", "\0a") ? "yes" : "no", "\n";
echo "starts-empty:", str_starts_with("", "") ? "yes" : "no", "\n";
echo "starts-large:", str_starts_with("ab" . str_repeat("c", 4096), "ab") ? "yes" : "no", "\n";
echo "starts-named:", str_starts_with(needle: "ab", haystack: "abcd") ? "yes" : "no", "\n";
$starts_ref = "abcd";
$starts_alias =& $starts_ref;
echo "starts-ref:", str_starts_with($starts_alias, "ab") ? "yes" : "no", "\n";
show_error("starts-arity", fn() => str_starts_with("abc"));
show_error("starts-type", fn() => str_starts_with([], "a"));

echo "ends-binary:", str_ends_with("abc\0", "c\0") ? "yes" : "no", "\n";
echo "ends-empty:", str_ends_with("", "") ? "yes" : "no", "\n";
echo "ends-large:", str_ends_with(str_repeat("c", 4096) . "yz", "yz") ? "yes" : "no", "\n";
echo "ends-named:", str_ends_with(needle: "cd", haystack: "abcd") ? "yes" : "no", "\n";
$ends_ref = "abcd";
$ends_alias =& $ends_ref;
echo "ends-ref:", str_ends_with($ends_alias, "cd") ? "yes" : "no", "\n";
show_error("ends-arity", fn() => str_ends_with("abc"));
show_error("ends-type", fn() => str_ends_with([], "a"));

echo "lower-binary:", strtolower("A\0Z"), "\n";
echo "lower-empty:", strtolower(""), "\n";
echo "lower-large:", substr(strtolower(str_repeat("A", 4096) . "Z"), -3), "\n";
echo "lower-named:", strtolower(string: "ABC"), "\n";
$lower_ref = "ABC";
$lower_alias =& $lower_ref;
echo "lower-ref:", strtolower($lower_alias), "\n";
show_error("lower-arity", fn() => strtolower());
show_error("lower-type", fn() => strtolower([]));
