<?php
// runtime-semantics: category=variables expect=pass php_ref_required=1
// Scalar locals copy on assignment and on return: later writes to the
// source must not be observable through the copy or the returned value.
function pick($n) {
    $inner = $n;
    $n = $n + 100;
    return $inner;
}

$a = 7;
$b = $a;
$a = 9;
echo $a, "|", $b, "\n";

$c = pick($b);
$b = 42;
echo $b, "|", $c, "\n";

$s = "left";
$t = $s;
$s = $s . "-changed";
echo $s, "|", $t, "\n";

$f = 1.5;
$g = $f;
$f = $f * 2.0;
echo $f, "|", $g, "\n";
