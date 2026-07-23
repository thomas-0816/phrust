<?php
// runtime-semantics: category=include_eval_autoload expect=pass php_ref_required=1

$callback = require __DIR__ . '/_data/external-fiber-closure-child.php';
$fiber = new Fiber($callback);

var_dump($fiber->start('start'));
var_dump($fiber->resume('resume'));
var_dump($fiber->getReturn());
