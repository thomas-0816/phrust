<?php
// oracle-probe: id=oracle-builtin-contract-function-proc-close-709b576e3e area=builtin_contract kind=function symbol=proc_close source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-proc-close-709b576e3e failure_category=builtin_contract
$name = "proc_close";
echo function_exists($name) ? "available\n" : "missing\n";
