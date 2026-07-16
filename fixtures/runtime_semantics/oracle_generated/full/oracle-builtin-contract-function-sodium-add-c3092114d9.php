<?php
// oracle-probe: id=oracle-builtin-contract-function-sodium-add-c3092114d9 area=builtin_contract kind=function symbol=sodium_add source=ext/sodium/libsodium.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-sodium-add-c3092114d9 failure_category=builtin_contract requires_ref_extension=sodium
$name = "sodium_add";
echo function_exists($name) ? "available\n" : "missing\n";
