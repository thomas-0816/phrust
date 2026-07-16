<?php
// oracle-probe: id=oracle-builtin-contract-function-hash-hmac-file-715c42b14b area=builtin_contract kind=function symbol=hash_hmac_file source=ext/hash/hash.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-hash-hmac-file-715c42b14b failure_category=builtin_contract requires_ref_extension=hash
$name = "hash_hmac_file";
echo function_exists($name) ? "available\n" : "missing\n";
