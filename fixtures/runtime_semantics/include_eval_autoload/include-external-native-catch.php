<?php
// runtime-semantics: category=include_eval_autoload expect=pass php_ref_required=1

require __DIR__ . '/_data/external-native-catch-child.php';

var_dump(external_native_catch_boundary(1));
var_dump(array_map('external_native_catch_boundary', array(1, 2)));
