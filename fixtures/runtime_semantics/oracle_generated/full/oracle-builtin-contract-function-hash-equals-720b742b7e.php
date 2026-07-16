<?php
// oracle-probe: id=oracle-builtin-contract-function-hash-equals-720b742b7e area=builtin_contract kind=function symbol=hash_equals source=ext/hash/hash.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-hash-equals-720b742b7e failure_category=builtin_contract requires_ref_extension=hash
$name = "hash_equals";
echo function_exists($name) ? "available\n" : "missing\n";
