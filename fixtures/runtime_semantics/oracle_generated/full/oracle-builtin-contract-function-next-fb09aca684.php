<?php
// oracle-probe: id=oracle-builtin-contract-function-next-fb09aca684 area=builtin_contract kind=function symbol=next source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-next-fb09aca684 failure_category=builtin_contract
$name = "next";
echo function_exists($name) ? "available\n" : "missing\n";
