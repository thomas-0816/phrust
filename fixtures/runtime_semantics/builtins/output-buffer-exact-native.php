<?php
// runtime-semantics: category=builtins expect=pass

function run_native_output_buffers(): array
{
    ob_start();
    echo "alpha\0";
    $contents = ob_get_contents();
    echo 'beta';
    $clean = ob_get_clean();

    ob_start();
    ob_start();
    echo 'flush';
    $flushed = ob_get_flush();
    $outer = ob_get_clean();

    return [$contents, $clean, $flushed, $outer, ob_get_level()];
}

$result = null;
for ($iteration = 0; $iteration < 32; $iteration++) {
    $result = run_native_output_buffers();
}

var_dump($result);
