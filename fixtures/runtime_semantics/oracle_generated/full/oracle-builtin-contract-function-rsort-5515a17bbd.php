<?php
// oracle-probe: id=oracle-builtin-contract-function-rsort-5515a17bbd area=builtin_contract kind=function symbol=rsort source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-rsort-5515a17bbd failure_category=builtin_contract
$name = "rsort";
echo function_exists($name) ? "available\n" : "missing\n";
