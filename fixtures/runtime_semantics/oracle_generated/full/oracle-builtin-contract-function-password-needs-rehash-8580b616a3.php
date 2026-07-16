<?php
// oracle-probe: id=oracle-builtin-contract-function-password-needs-rehash-8580b616a3 area=builtin_contract kind=function symbol=password_needs_rehash source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-password-needs-rehash-8580b616a3 failure_category=builtin_contract
$name = "password_needs_rehash";
echo function_exists($name) ? "available\n" : "missing\n";
