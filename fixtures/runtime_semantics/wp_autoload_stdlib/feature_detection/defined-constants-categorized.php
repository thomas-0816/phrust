<?php
// runtime-semantics: category=wp_autoload_stdlib expect=pass
const PACK_B_CATEGORIZED_CONST = 1;
define("PACK_B_CATEGORIZED_DEFINE", 2);

$constants = get_defined_constants(true);
echo array_key_exists("PHP_VERSION", $constants["Core"]) ? "core=yes\n" : "core=no\n";
echo array_key_exists("PACK_B_CATEGORIZED_CONST", $constants["user"]) ? "user-const=yes\n" : "user-const=no\n";
echo array_key_exists("PACK_B_CATEGORIZED_DEFINE", $constants["user"]) ? "user-define=yes\n" : "user-define=no\n";
