<?php

ob_start();
$result = phpinfo(INFO_MODULES);
$output = ob_get_clean();
echo $result ? "true\n" : "false\n";
echo $output;
