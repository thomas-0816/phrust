<?php
// runtime-semantics: category=include_eval_autoload expect=pass
require __DIR__ . '/_data/external-constructor-state-child.php';

$user = 'app';
$password = '';
$database = 'cms';
$host = 'database.test';
$state = new ExternalConstructorState($user, $password, $database, $host);
$state->show();
