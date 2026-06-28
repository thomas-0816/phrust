<?php
if ($_SERVER["PATH_INFO"] == "/users/42") {
    echo "front=user|", $_SERVER["SCRIPT_NAME"], "|", $_SERVER["PHP_SELF"], "|", $_SERVER["PATH_INFO"], "|", $_SERVER["REQUEST_URI"], "\n";
} else {
    http_response_code(404);
    echo "front=missing|", $_SERVER["REQUEST_URI"], "\n";
}
