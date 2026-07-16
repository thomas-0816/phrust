<?php
// oracle-probe: id=oracle-builtin-contract-function-sys-get-temp-dir-6a7f6b4170 area=builtin_contract kind=function symbol=sys_get_temp_dir source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-sys-get-temp-dir-6a7f6b4170 failure_category=builtin_contract
$name = "sys_get_temp_dir";
echo function_exists($name) ? "available\n" : "missing\n";
