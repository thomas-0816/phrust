<?php
echo implode("|", [
    "admin-index",
    $_SERVER["REQUEST_URI"],
    $_SERVER["SCRIPT_NAME"],
    $_SERVER["PHP_SELF"],
    $_SERVER["PATH_INFO"] ?? "",
]) . "\n";
