<?php

set_error_handler(function ($level, $message, $file, $line) {
    echo "handled:", $level, ":", $message, ":", ($line > 0 ? "line" : "missing"), "\n";
    return true;
});

var_dump(trigger_error("handled-native", E_USER_WARNING));
restore_error_handler();
