<?php
// oracle-probe: id=oracle-builtin-contract-function-sodium-crypto-aead-xchacha20poly1305-ietf-encrypt-6a929088de area=builtin_contract kind=function symbol=sodium_crypto_aead_xchacha20poly1305_ietf_encrypt source=ext/sodium/libsodium.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-sodium-crypto-aead-xchacha20poly1305-ietf-encrypt-6a929088de failure_category=builtin_contract requires_ref_extension=sodium
try {
    $result = \sodium_crypto_aead_xchacha20poly1305_ietf_encrypt();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
