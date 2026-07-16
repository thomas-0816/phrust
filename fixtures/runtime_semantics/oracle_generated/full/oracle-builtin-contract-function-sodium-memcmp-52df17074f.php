<?php
// oracle-probe: id=oracle-builtin-contract-function-sodium-memcmp-52df17074f area=builtin_contract kind=function symbol=sodium_memcmp source=ext/sodium/libsodium.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-sodium-memcmp-52df17074f failure_category=builtin_contract requires_ref_extension=sodium
$name = "sodium_memcmp";
echo function_exists($name) ? "available\n" : "missing\n";
