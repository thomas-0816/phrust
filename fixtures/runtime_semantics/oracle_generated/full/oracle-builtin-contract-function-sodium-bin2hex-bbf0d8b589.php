<?php
// oracle-probe: id=oracle-builtin-contract-function-sodium-bin2hex-bbf0d8b589 area=builtin_contract kind=function symbol=sodium_bin2hex source=ext/sodium/libsodium.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-sodium-bin2hex-bbf0d8b589 failure_category=builtin_contract requires_ref_extension=sodium
$name = "sodium_bin2hex";
echo function_exists($name) ? "available\n" : "missing\n";
