<?php
// oracle-probe: id=oracle-builtin-contract-function-quotemeta-3e6dcb9f82 area=builtin_contract kind=function symbol=quotemeta source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-quotemeta-3e6dcb9f82 failure_category=builtin_contract
$name = "quotemeta";
echo function_exists($name) ? "available\n" : "missing\n";
