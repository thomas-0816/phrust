<?php
ob_start();
echo "buffered";
$body = ob_get_clean();
http_response_code(302);
header('Location: /wp-admin/install.php');
header('X-WP-Fixture: response');
setcookie('install', '1');
echo $body;
