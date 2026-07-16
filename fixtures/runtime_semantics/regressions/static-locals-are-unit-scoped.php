<?php
// runtime-semantics: expect=pass regression_category=statics reference_behavior=stdout:a|42|a|43 regression_case=lazy-native-compilation

require __DIR__ . '/_data/static-local-unit-a.php';
require __DIR__ . '/_data/static-local-unit-b.php';

echo static_local_unit_a(), '|', static_local_unit_b(), '|';
echo static_local_unit_a(), '|', static_local_unit_b(), "\n";
