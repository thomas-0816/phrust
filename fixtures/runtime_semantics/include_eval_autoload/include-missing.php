<?php
// runtime-semantics: category=include_eval_autoload expect=pass
echo "before|";
include (__DIR__ . "/_data/lib/missing.php");
echo "after\n";
