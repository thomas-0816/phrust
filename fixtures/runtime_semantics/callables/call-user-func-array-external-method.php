<?php
require __DIR__ . "/_data/external-callable-class.php";

$target = new ExternalCallableTarget();
echo call_user_func_array([$target, "decorate"], ["external"]), "\n";
