<?php
require __DIR__ . "/../shared/bootstrap.php";

$path = $_SERVER["PATH_INFO"] ?? "/";
$route = "front";
if ($path === "/" || $path === "") {
    $route = "home";
}

echo implode("|", [
    $route,
    $_SERVER["REQUEST_METHOD"],
    $_SERVER["REQUEST_URI"],
    $_SERVER["DOCUMENT_URI"],
    $_SERVER["SCRIPT_NAME"],
    $_SERVER["PHP_SELF"],
    $_SERVER["PATH_INFO"] ?? "",
    $_SERVER["QUERY_STRING"],
    $_SERVER["DOCUMENT_ROOT"],
    $_SERVER["SCRIPT_FILENAME"],
    $_SERVER["REQUEST_SCHEME"],
    $_SERVER["HTTPS"],
    $_SERVER["HTTP_HOST"],
    $_SERVER["SERVER_NAME"],
    $_SERVER["REMOTE_ADDR"],
    $bootstrap_value,
]) . "\n";
