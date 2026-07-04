<?php
// runtime-semantics: category=objects expect=pass php_ref_required=1
// Object assignment copies the handle, not the object: writes through
// either variable are visible through both, including via function
// parameters and return values.
class Counter {
    public $n = 0;
}

$a = new Counter();
$b = $a;
$b->n = 5;
echo $a->n, "|", $b->n, "|", ($a === $b ? "same" : "different"), "\n";

function touch_counter($c) {
    $c->n = $c->n + 1;
    return $c;
}

$c = touch_counter($a);
echo $a->n, "|", ($c === $a ? "same" : "different"), "\n";

$d = new Counter();
$e = $d;
$d = new Counter();
$d->n = 100;
echo $e->n, "|", $d->n, "\n";
