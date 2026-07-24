<?php

require __DIR__ . '/lib/global-binding-missing.php';

var_dump(isset($GLOBALS['native_missing_method_global']));
var_dump(isset($GLOBALS['native_missing_function_global']));
(new MissingGlobalBindingTarget())->run();
bind_missing_function_global(__FILE__);
