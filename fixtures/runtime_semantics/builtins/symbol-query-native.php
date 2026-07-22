<?php

interface NativeSymbolQueryInterface {}
trait NativeSymbolQueryTrait {}
enum NativeSymbolQueryEnum { case Ready; }

class NativeSymbolQueryClass
{
    public string $declared;

    public function method(): void {}
}

function native_symbol_query(
    string $constant,
    string $function,
    object $object,
): array {
    return [
        defined($constant),
        function_exists($function),
        class_exists('NativeSymbolQueryClass', false),
        interface_exists('NativeSymbolQueryInterface', false),
        trait_exists('NativeSymbolQueryTrait', false),
        enum_exists('NativeSymbolQueryEnum', false),
        method_exists('NativeSymbolQueryClass', 'method'),
        property_exists('NativeSymbolQueryClass', 'declared'),
        property_exists($object, 'dynamic'),
        class_exists('NativeSymbolQueryMissing', false),
    ];
}

define('NATIVE_SYMBOL_QUERY_VALUE', 42);
function native_symbol_query_user_function(): void {}

$object = new stdClass();
$object->dynamic = 1;
$result = null;
for ($iteration = 0; $iteration < 32; $iteration++) {
    $result = native_symbol_query(
        'NATIVE_SYMBOL_QUERY_VALUE',
        'native_symbol_query_user_function',
        $object,
    );
}
var_dump($result);
