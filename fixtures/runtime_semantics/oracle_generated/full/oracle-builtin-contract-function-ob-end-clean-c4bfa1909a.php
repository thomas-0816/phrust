<?php
// oracle-probe: id=oracle-builtin-contract-function-ob-end-clean-c4bfa1909a area=builtin_contract kind=function symbol=ob_end_clean source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-ob-end-clean-c4bfa1909a failure_category=builtin_contract
$name = "ob_end_clean";
echo function_exists($name) ? "available\n" : "missing\n";
