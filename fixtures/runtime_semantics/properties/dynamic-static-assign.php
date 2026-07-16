<?php

class DynamicStaticTarget
{
    public static mixed $validator = null;
}

$class = DynamicStaticTarget::class;
$class::$validator = static function (string $value): bool {
    return $value !== '';
};

var_dump((DynamicStaticTarget::$validator)('ok'));
var_dump((DynamicStaticTarget::$validator)(''));
