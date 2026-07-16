<?php

function runtime_constant_default_child($value = RUNTIME_DEFAULT_MODE): void
{
    echo $value, "\n";
}

function runtime_constant_default_nested($value = RUNTIME_DEFAULT_MODE): void
{
    echo 'nested:', $value, "\n";
}

function runtime_constant_default_wrapper(): void
{
    runtime_constant_default_nested();
}

function runtime_default_child_padding_00(): void {}
function runtime_default_child_padding_01(): void {}
function runtime_default_child_padding_02(): void {}
function runtime_default_child_padding_03(): void {}
function runtime_default_child_padding_04(): void {}
function runtime_default_child_padding_05(): void {}
function runtime_default_child_padding_06(): void {}
function runtime_default_child_padding_07(): void {}
function runtime_default_child_padding_08(): void {}
function runtime_default_child_padding_09(): void {}
function runtime_default_child_padding_10(): void {}
function runtime_default_child_padding_11(): void {}
function runtime_default_child_padding_12(): void {}
function runtime_default_child_padding_13(): void {}
function runtime_default_child_padding_14(): void {}
function runtime_default_child_padding_15(): void {}
