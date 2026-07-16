<?php
// oracle-probe: id=oracle-builtin-contract-function-mhash-keygen-s2k-8d9926a46a area=builtin_contract kind=function symbol=mhash_keygen_s2k source=ext/hash/hash.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-mhash-keygen-s2k-8d9926a46a failure_category=builtin_contract requires_ref_extension=hash
$name = "mhash_keygen_s2k";
echo function_exists($name) ? "available\n" : "missing\n";
