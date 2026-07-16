<?php
require __DIR__ . '/_data/returned-bound-closure-child.php';

$raw = ReturnedBoundClosureFactory::makeRaw('raw');
var_dump($raw instanceof Closure);
echo call_user_func($raw), "\n";

$callback = ReturnedBoundClosureFactory::makeBound('captured');
var_dump($callback instanceof Closure);
echo call_user_func($callback), "\n";
