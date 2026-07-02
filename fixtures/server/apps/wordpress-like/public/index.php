<?php
require __DIR__ . "/../shared/wp-load.php";

$path = $_SERVER["PATH_INFO"] ?? "/";
$query = $_GET["preview"] ?? "0";

header("X-WordPress-Like: hotpath");
setcookie("wp_like_seen", "1");

echo wp_like_render_request($path, $query);
