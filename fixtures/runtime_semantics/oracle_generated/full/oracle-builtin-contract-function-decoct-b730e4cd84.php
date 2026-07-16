<?php
// oracle-probe: id=oracle-builtin-contract-function-decoct-b730e4cd84 area=builtin_contract kind=function symbol=decoct source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-decoct-b730e4cd84 failure_category=builtin_contract
$name = "decoct";
echo function_exists($name) ? "available\n" : "missing\n";
