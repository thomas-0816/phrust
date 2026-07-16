<?php
// runtime-semantics: expect=pass regression_category=errors reference_behavior=stdout:handled:2|false|after regression_case=native-builtin-diagnostic-handler
set_error_handler(function ($errno, $message) {
    echo "handled:", $errno, "\n";
    return true;
});

var_dump(hex2bin('abc'));
restore_error_handler();
echo "after\n";
