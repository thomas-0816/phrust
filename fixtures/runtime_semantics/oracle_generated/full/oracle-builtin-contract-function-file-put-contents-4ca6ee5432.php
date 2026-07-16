<?php
// oracle-probe: id=oracle-builtin-contract-function-file-put-contents-4ca6ee5432 area=builtin_contract kind=function symbol=file_put_contents source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-file-put-contents-4ca6ee5432 failure_category=builtin_contract
$name = "file_put_contents";
echo function_exists($name) ? "available\n" : "missing\n";
