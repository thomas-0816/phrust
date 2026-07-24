<?php
// runtime-semantics: expect=pass regression_category=arrays reference_behavior=stdout:exact-auto-index-state regression_case=direct-array-auto-index-state

function append_after_unset(array $array): array {
    unset($array[5]);
    $array[] = "unset";
    return array_keys($array);
}

function append_after_pop(array $array): array {
    array_pop($array);
    $array[] = "pop";
    return array_keys($array);
}

echo implode(",", append_after_unset([5 => "value"])), "\n";
echo implode(",", append_after_pop([-2 => "value"])), "\n";
echo implode(",", append_after_pop([5 => "value"])), "\n";

function maximum_auto_index(): void {
    $maximum = [PHP_INT_MAX => "value"];
    unset($maximum[PHP_INT_MAX]);
    $maximum[] = "reused";
    echo array_key_first($maximum), "\n";

    try {
        $maximum[] = "overflow";
    } catch (Error $error) {
        echo $error->getMessage(), "\n";
    }
}

maximum_auto_index();
