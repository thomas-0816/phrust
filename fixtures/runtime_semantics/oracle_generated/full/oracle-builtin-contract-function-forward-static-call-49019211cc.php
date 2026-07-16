<?php
// oracle-probe: id=oracle-builtin-contract-function-forward-static-call-49019211cc area=builtin_contract kind=function symbol=forward_static_call source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-forward-static-call-49019211cc failure_category=builtin_contract
$name = "forward_static_call";
echo function_exists($name) ? "available\n" : "missing\n";
