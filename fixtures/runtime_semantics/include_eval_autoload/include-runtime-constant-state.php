<?php

define('PARENT_RUNTIME_CONSTANT', 'parent-visible');

include __DIR__ . '/_data/lib/runtime-constant-child.php';

echo CHILD_RUNTIME_CONSTANT, "\n";
