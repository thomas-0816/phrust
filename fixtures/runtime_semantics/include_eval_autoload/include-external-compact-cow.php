<?php
// runtime-semantics: category=include_eval_autoload expect=pass

require __DIR__ . '/_data/external-compact-processor.php';
require __DIR__ . '/_data/external-compact-caller.php';

external_compact_cow();
