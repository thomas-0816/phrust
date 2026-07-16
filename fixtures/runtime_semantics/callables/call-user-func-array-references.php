<?php
function invoke_referenced_callback(array &$entry, array &$arguments): mixed {
    return call_user_func_array($entry["callback"], $arguments);
}

$entry = ["callback" => "strtoupper"];
$arguments = ["wordpress"];
echo invoke_referenced_callback($entry, $arguments), "\n";
