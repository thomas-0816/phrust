<?php
// runtime-semantics: expect=pass

define('RUNTIME_DEFAULT_MODE', 'runtime-default-ok');

function runtime_constant_default_local($value = RUNTIME_DEFAULT_MODE): void
{
    echo 'local:', $value, "\n";
}

function runtime_default_padding_00(): void {}
function runtime_default_padding_01(): void {}
function runtime_default_padding_02(): void {}
function runtime_default_padding_03(): void {}
function runtime_default_padding_04(): void {}
function runtime_default_padding_05(): void {}
function runtime_default_padding_06(): void {}
function runtime_default_padding_07(): void {}
function runtime_default_padding_08(): void {}
function runtime_default_padding_09(): void {}
function runtime_default_padding_10(): void {}
function runtime_default_padding_11(): void {}
function runtime_default_padding_12(): void {}
function runtime_default_padding_13(): void {}
function runtime_default_padding_14(): void {}
function runtime_default_padding_15(): void {}

include __DIR__ . '/_data/runtime-constant-default-child.php';

runtime_constant_default_local();
runtime_constant_default_child();
runtime_constant_default_wrapper();
