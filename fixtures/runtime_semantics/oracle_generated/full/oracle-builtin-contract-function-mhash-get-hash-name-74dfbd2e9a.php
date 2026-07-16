<?php
// oracle-probe: id=oracle-builtin-contract-function-mhash-get-hash-name-74dfbd2e9a area=builtin_contract kind=function symbol=mhash_get_hash_name source=ext/hash/hash.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-mhash-get-hash-name-74dfbd2e9a failure_category=builtin_contract requires_ref_extension=hash
$name = "mhash_get_hash_name";
echo function_exists($name) ? "available\n" : "missing\n";
