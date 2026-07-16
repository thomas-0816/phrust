<?php
// oracle-probe: id=oracle-builtin-contract-function-hash-update-stream-dc05f1dfc2 area=builtin_contract kind=function symbol=hash_update_stream source=ext/hash/hash.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-hash-update-stream-dc05f1dfc2 failure_category=builtin_contract requires_ref_extension=hash
$name = "hash_update_stream";
echo function_exists($name) ? "available\n" : "missing\n";
