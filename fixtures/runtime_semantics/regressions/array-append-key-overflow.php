<?php
// runtime-semantics: expect=pass regression_category=arrays reference_behavior=stdout:two-caught-overflows regression_case=array-append-key-overflow

try {
    $array = [PHP_INT_MAX => 42, 'overflow'];
} catch (Error $error) {
    echo $error->getMessage(), "\n";
}

try {
    array_merge_recursive(
        ['value' => [PHP_INT_MAX => null]],
        ['value' => 'overflow'],
    );
} catch (Throwable $error) {
    echo $error->getMessage(), "\n";
}
