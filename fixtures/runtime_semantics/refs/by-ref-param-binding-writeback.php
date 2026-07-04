<?php
// runtime-semantics: category=refs expect=pass php_ref_required=1
// By-ref parameter binding: the callee's writes reach the caller, the
// initial value seen by the callee is the caller's value at call time,
// and aliases created before the call observe the write-back too.
function bump(&$x, $by) {
    $seen = $x;
    $x = $x + $by;
    return $seen;
}

$n = 5;
$alias =& $n;
$seen = bump($n, 10);
echo $seen, "|", $n, "|", $alias, "\n";

$arr = [1, 2];
function grow(&$a) {
    $a[] = 3;
}
grow($arr);
echo count($arr), "|", $arr[2], "\n";

function reset_to(&$v, $to) {
    $v = $to;
}
$s = "before";
reset_to($s, "after");
echo $s, "\n";
