<?php
// oracle-probe: id=oracle-builtin-contract-function-natsort-84155596dd area=builtin_contract kind=function symbol=natsort source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-natsort-84155596dd failure_category=builtin_contract
$name = "natsort";
echo function_exists($name) ? "available\n" : "missing\n";
