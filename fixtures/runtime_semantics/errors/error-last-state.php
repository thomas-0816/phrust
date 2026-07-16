<?php

var_dump(error_get_last());
echo $missing;
$error = error_get_last();
echo $error['type'], "\n";
echo $error['message'], "\n";
echo basename($error['file']), ':', $error['line'], "\n";
error_clear_last();
var_dump(error_get_last());
