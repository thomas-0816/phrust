<?php
require __DIR__ . "/../shared/app-load.php";

$path = $_SERVER["PATH_INFO"] ?? "/";
$query = $_GET["preview"] ?? "0";

header("X-App-Hotpath: hotpath");
setcookie("app_hotpath_seen", "1");

echo app_render_request($path, $query);
