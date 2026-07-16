<?php
// oracle-probe: id=oracle-builtin-contract-function-sodium-crypto-aead-xchacha20poly1305-ietf-encrypt-a03c5fd0be area=builtin_contract kind=function symbol=sodium_crypto_aead_xchacha20poly1305_ietf_encrypt source=ext/sodium/libsodium.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-sodium-crypto-aead-xchacha20poly1305-ietf-encrypt-a03c5fd0be failure_category=builtin_contract requires_ref_extension=sodium
$name = "sodium_crypto_aead_xchacha20poly1305_ietf_encrypt";
echo function_exists($name) ? "available\n" : "missing\n";
