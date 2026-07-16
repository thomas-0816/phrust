<?php
// oracle-probe: id=oracle-builtin-contract-function-sodium-pad-d9450a9047 area=builtin_contract kind=function symbol=sodium_pad source=ext/sodium/libsodium.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-sodium-pad-d9450a9047 failure_category=builtin_contract requires_ref_extension=sodium
$name = "sodium_pad";
echo function_exists($name) ? "available\n" : "missing\n";
