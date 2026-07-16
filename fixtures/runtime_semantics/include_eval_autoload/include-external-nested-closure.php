<?php
// runtime-semantics: category=include_eval_autoload expect=pass

require __DIR__ . '/_data/external-nested-closure-child.php';

$container = external_nested_closure_container();
echo call_user_func_array($container['callbacks'][0], array('closure')), "\n";

$scoped = new ExternalClosurePrivateScope();
echo $scoped->reduce(array('private', 'scope')), "\n";
