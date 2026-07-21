<?php
// runtime-fixture: kind=valid

function include_function_scope_version() {
    require __DIR__ . '/lib/function-scope-version.php';
    return $fixture_version;
}

echo include_function_scope_version(), "\n";
