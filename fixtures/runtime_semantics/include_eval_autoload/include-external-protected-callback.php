<?php
// runtime-semantics: category=include_eval_autoload expect=pass

require __DIR__ . '/_data/external-protected-callback-child.php';

echo ExternalProtectedCallback::run('callback'), "\n";
