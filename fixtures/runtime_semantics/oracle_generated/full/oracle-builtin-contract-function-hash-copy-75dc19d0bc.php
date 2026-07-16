<?php
// oracle-probe: id=oracle-builtin-contract-function-hash-copy-75dc19d0bc area=builtin_contract kind=function symbol=hash_copy source=ext/hash/hash.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-hash-copy-75dc19d0bc failure_category=builtin_contract requires_ref_extension=hash
$name = "hash_copy";
echo function_exists($name) ? "available\n" : "missing\n";
