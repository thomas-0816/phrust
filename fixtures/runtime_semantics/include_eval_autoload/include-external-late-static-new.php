<?php
// runtime-semantics: category=include_eval_autoload expect=pass

require __DIR__ . '/_data/external-late-static-new-child.php';

$value = ExternalLateStaticChild::create();
var_dump($value instanceof ExternalLateStaticChild);
