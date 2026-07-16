<?php
// oracle-probe: id=oracle-builtin-contract-function-sodium-crypto-kdf-derive-from-key-bf5ade9120 area=builtin_contract kind=function symbol=sodium_crypto_kdf_derive_from_key source=ext/sodium/libsodium.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-sodium-crypto-kdf-derive-from-key-bf5ade9120 failure_category=builtin_contract requires_ref_extension=sodium
$name = "sodium_crypto_kdf_derive_from_key";
echo function_exists($name) ? "available\n" : "missing\n";
