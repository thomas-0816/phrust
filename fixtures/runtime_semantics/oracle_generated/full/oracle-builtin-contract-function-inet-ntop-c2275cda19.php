<?php
// oracle-probe: id=oracle-builtin-contract-function-inet-ntop-c2275cda19 area=builtin_contract kind=function symbol=inet_ntop source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-inet-ntop-c2275cda19 failure_category=builtin_contract
$name = "inet_ntop";
echo function_exists($name) ? "available\n" : "missing\n";
