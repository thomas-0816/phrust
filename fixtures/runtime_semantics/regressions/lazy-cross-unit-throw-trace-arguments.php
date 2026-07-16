<?php
// runtime-semantics: expect=pass regression_category=exceptions reference_behavior=stdout:large-unit trace survives released parameter regression_case=lazy-native-compilation

require __DIR__ . '/_data/lazy-cross-unit-nested-method-class.php';

try {
    (new LazyCrossUnitNestedMethodHook())->invoke_throwing(array('first'));
} catch (RuntimeException $exception) {
    echo $exception->getMessage(), "\n";
}
