<?php
// runtime-semantics: category=wp_autoload_stdlib expect=pass
$items = ["first" => " Alpha ", "second" => "Beta"];
ini_set("include_path", ".");
echo trim($items["first"]), "|";
echo strtolower($items["second"]), "|";
echo implode(",", array_keys($items)), "|";
echo str_contains("PackB runtime", "runtime") ? "contains" : "missing";
echo "|", ini_get("include_path"), "\n";
