<?php
// oracle-probe: id=oracle-builtin-contract-function-popen-bce6d12edc area=builtin_contract kind=function symbol=popen source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-popen-bce6d12edc failure_category=builtin_contract
$name = "popen";
echo function_exists($name) ? "available\n" : "missing\n";
