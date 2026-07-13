<?php

if (true) {
    function DynamicIndexedConditional(): string
    {
        return 'conditional';
    }
}

eval('function DynamicIndexedEval(): string { return "eval"; }');

class DynamicIndexedOriginal
{
    public const VALUE = 'class';
}

class_alias(DynamicIndexedOriginal::class, 'DynamicIndexedAlias');
define('DYNAMIC_INDEXED_CONSTANT', 'constant');

spl_autoload_register(static function (string $name): void {
    if ($name === 'DynamicIndexedAutoloaded') {
        eval('class DynamicIndexedAutoloaded { public const VALUE = "autoload"; }');
    }
});

echo dynamicindexedconditional(), '|';
echo DYNAMICINDEXEDEVAL(), '|';
echo DynamicIndexedAlias::VALUE, '|';
echo DynamicIndexedAutoloaded::VALUE, '|';
echo DYNAMIC_INDEXED_CONSTANT, '|';
var_export(defined('dynamic_indexed_constant'));
echo "\n";
