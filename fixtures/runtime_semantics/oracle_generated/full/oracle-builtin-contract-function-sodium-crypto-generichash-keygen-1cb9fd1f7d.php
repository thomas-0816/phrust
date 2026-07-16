<?php
// oracle-probe: id=oracle-builtin-contract-function-sodium-crypto-generichash-keygen-1cb9fd1f7d area=builtin_contract kind=function symbol=sodium_crypto_generichash_keygen source=ext/sodium/libsodium.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-sodium-crypto-generichash-keygen-1cb9fd1f7d failure_category=builtin_contract requires_ref_extension=sodium
$name = "sodium_crypto_generichash_keygen";
echo function_exists($name) ? "available\n" : "missing\n";
