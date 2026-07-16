<?php
// oracle-probe: id=oracle-builtin-contract-function-get-cfg-var-51fa62584a area=builtin_contract kind=function symbol=get_cfg_var source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-get-cfg-var-51fa62584a failure_category=builtin_contract
$name = "get_cfg_var";
echo function_exists($name) ? "available\n" : "missing\n";
