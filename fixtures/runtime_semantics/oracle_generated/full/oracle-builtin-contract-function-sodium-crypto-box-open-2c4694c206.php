<?php
// oracle-probe: id=oracle-builtin-contract-function-sodium-crypto-box-open-2c4694c206 area=builtin_contract kind=function symbol=sodium_crypto_box_open source=ext/sodium/libsodium.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-sodium-crypto-box-open-2c4694c206 failure_category=builtin_contract requires_ref_extension=sodium
$name = "sodium_crypto_box_open";
echo function_exists($name) ? "available\n" : "missing\n";
