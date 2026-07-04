<?php
// runtime-semantics: category=functions expect=pass php_ref_required=1
// Returned arrays/strings are value copies (later callee-side or
// caller-side writes stay isolated); returned objects share identity.
class Holder {
    public $items = [];
    public $tag = "start";
}

function make_array() {
    $a = [1, 2, 3];
    return $a;
}

function make_string() {
    $s = "base";
    return $s;
}

$holder = new Holder();

function give_object($h) {
    $h->tag = "given";
    return $h;
}

$arr1 = make_array();
$arr2 = make_array();
$arr1[] = 4;
echo count($arr1), "|", count($arr2), "\n";

$s1 = make_string();
$s2 = $s1;
$s1 .= "-more";
echo $s1, "|", $s2, "\n";

$same = give_object($holder);
$same->items[] = "x";
echo $holder->tag, "|", count($holder->items), "|", ($same === $holder ? "same" : "different"), "\n";
