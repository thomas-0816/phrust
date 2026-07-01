<?php
require __DIR__ . "/../../shared/bootstrap.php";

echo implode("|", [
    "install",
    $_SERVER["REQUEST_URI"],
    $_SERVER["DOCUMENT_URI"],
    $_SERVER["SCRIPT_NAME"],
    $_SERVER["PHP_SELF"],
    $_SERVER["PATH_INFO"] ?? "",
    $_SERVER["QUERY_STRING"],
    $bootstrap_value,
]) . "\n";
