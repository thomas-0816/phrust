<?php
// oracle-probe: id=oracle-builtin-contract-function-is-uploaded-file-1d292be82b area=builtin_contract kind=function symbol=is_uploaded_file source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-is-uploaded-file-1d292be82b failure_category=builtin_contract
$name = "is_uploaded_file";
echo function_exists($name) ? "available\n" : "missing\n";
