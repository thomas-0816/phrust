<?php
// oracle-probe: id=oracle-builtin-contract-function-sodium-crypto-shorthash-keygen-69a639fd4c area=builtin_contract kind=function symbol=sodium_crypto_shorthash_keygen source=ext/sodium/libsodium.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-sodium-crypto-shorthash-keygen-69a639fd4c failure_category=builtin_contract requires_ref_extension=sodium
$name = "sodium_crypto_shorthash_keygen";
echo function_exists($name) ? "available\n" : "missing\n";
