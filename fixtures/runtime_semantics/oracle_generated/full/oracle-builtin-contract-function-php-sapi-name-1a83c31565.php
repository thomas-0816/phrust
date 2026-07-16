<?php
// oracle-probe: id=oracle-builtin-contract-function-php-sapi-name-1a83c31565 area=builtin_contract kind=function symbol=php_sapi_name source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-php-sapi-name-1a83c31565 failure_category=builtin_contract
$name = "php_sapi_name";
echo function_exists($name) ? "available\n" : "missing\n";
