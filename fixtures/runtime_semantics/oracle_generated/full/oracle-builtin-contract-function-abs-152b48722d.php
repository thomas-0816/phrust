<?php
// oracle-probe: id=oracle-builtin-contract-function-abs-152b48722d area=builtin_contract kind=function symbol=abs source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-abs-152b48722d failure_category=builtin_contract
$name = "abs";
echo function_exists($name) ? "available\n" : "missing\n";
