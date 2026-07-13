<?php
function g() {
    yield 'foo';
    yield 'bar';
    yield 5 => 'rab';
    yield 'oof';
}
foreach (g() as $k => $v) { echo "$k => $v\n"; }
function inner() { yield 1; yield 2; }
function outer() { yield 0; yield from inner(); yield 3; }
foreach (outer() as $k => $v) { echo "$k:$v\n"; }
$gen = (function() { yield 'a' => 1; yield 2; })();
foreach ($gen as $k => $v) { var_dump($k, $v); }
