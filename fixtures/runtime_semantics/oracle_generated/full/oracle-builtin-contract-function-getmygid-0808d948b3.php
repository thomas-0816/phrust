<?php
// oracle-probe: id=oracle-builtin-contract-function-getmygid-0808d948b3 area=builtin_contract kind=function symbol=getmygid source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-getmygid-0808d948b3 failure_category=builtin_contract
$name = "getmygid";
echo function_exists($name) ? "available\n" : "missing\n";
