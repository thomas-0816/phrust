<?php
// runtime-semantics: category=constants expect=pass

const NATIVE_INVENTORY_CONST = "const-value";
define("NATIVE_INVENTORY_DEFINE", 42);

function run_native_constant_inventory(): array
{
    $flat = get_defined_constants();
    $categorized = get_defined_constants(true);

    return [
        array_key_exists("PHP_VERSION", $flat),
        array_key_exists("PHP_VERSION", $categorized["Core"]),
        $flat["NATIVE_INVENTORY_CONST"],
        $flat["NATIVE_INVENTORY_DEFINE"],
        $categorized["user"]["NATIVE_INVENTORY_CONST"],
        $categorized["user"]["NATIVE_INVENTORY_DEFINE"],
        constant("NATIVE_INVENTORY_CONST"),
        constant("NATIVE_INVENTORY_DEFINE"),
    ];
}

$result = null;
for ($iteration = 0; $iteration < 32; $iteration++) {
    $result = run_native_constant_inventory();
}
var_dump($result);
