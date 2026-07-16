<?php
// oracle-probe: id=oracle-builtin-contract-function-inet-pton-965306a3cc area=builtin_contract kind=function symbol=inet_pton source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-inet-pton-965306a3cc failure_category=builtin_contract
$name = "inet_pton";
echo function_exists($name) ? "available\n" : "missing\n";
