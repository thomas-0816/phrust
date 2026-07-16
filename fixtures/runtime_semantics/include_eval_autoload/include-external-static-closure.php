<?php
// runtime-semantics: expect=pass

include __DIR__ . '/_data/external-static-closure-child.php';

use Fixture\ExternalStaticClosure\ValidatorHost;

ValidatorHost::$validator = static function ($value): string {
    return 'closure:' . $value;
};

var_dump(ValidatorHost::validate('ok'));
