<?php
// oracle-probe: id=oracle-builtin-contract-function-move-uploaded-file-9a7603ff39 area=builtin_contract kind=function symbol=move_uploaded_file source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-move-uploaded-file-9a7603ff39 failure_category=builtin_contract
$name = "move_uploaded_file";
echo function_exists($name) ? "available\n" : "missing\n";
