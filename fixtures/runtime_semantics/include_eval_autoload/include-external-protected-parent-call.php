<?php
// runtime-semantics: expect=pass

include __DIR__ . '/_data/external-protected-base.php';
include __DIR__ . '/_data/external-protected-child.php';

$value = new ExternalProtectedChild();
echo $value->expose(), "\n";
echo $value->exposeConstant(), "\n";
echo $value->callOptional(), "\n";
