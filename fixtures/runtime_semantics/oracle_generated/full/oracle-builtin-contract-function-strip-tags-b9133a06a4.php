<?php
// oracle-probe: id=oracle-builtin-contract-function-strip-tags-b9133a06a4 area=builtin_contract kind=function symbol=strip_tags source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-strip-tags-b9133a06a4 failure_category=builtin_contract
$name = "strip_tags";
echo function_exists($name) ? "available\n" : "missing\n";
