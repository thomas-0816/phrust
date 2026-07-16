<?php
// oracle-probe: id=oracle-builtin-contract-function-hash-file-4fe93d6767 area=builtin_contract kind=function symbol=hash_file source=ext/hash/hash.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-hash-file-4fe93d6767 failure_category=builtin_contract requires_ref_extension=hash
$name = "hash_file";
echo function_exists($name) ? "available\n" : "missing\n";
