<?php
// oracle-probe: id=oracle-builtin-contract-function-sodium-crypto-aead-xchacha20poly1305-ietf-decrypt-803f7d30c7 area=builtin_contract kind=function symbol=sodium_crypto_aead_xchacha20poly1305_ietf_decrypt source=ext/sodium/libsodium.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-sodium-crypto-aead-xchacha20poly1305-ietf-decrypt-803f7d30c7 failure_category=builtin_contract requires_ref_extension=sodium
$name = "sodium_crypto_aead_xchacha20poly1305_ietf_decrypt";
echo function_exists($name) ? "available\n" : "missing\n";
