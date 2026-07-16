<?php
// oracle-probe: id=oracle-builtin-contract-function-php-uname-6d09b71cb2 area=builtin_contract kind=function symbol=php_uname source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-php-uname-6d09b71cb2 failure_category=builtin_contract
$name = "php_uname";
echo function_exists($name) ? "available\n" : "missing\n";
