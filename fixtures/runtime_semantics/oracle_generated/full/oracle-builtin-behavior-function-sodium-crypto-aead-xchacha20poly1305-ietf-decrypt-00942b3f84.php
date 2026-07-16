<?php
// oracle-probe: id=oracle-builtin-behavior-function-sodium-crypto-aead-xchacha20poly1305-ietf-decrypt-00942b3f84 area=builtin_behavior kind=function symbol=sodium_crypto_aead_xchacha20poly1305_ietf_decrypt source=ext/sodium/libsodium.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-behavior-function-sodium-crypto-aead-xchacha20poly1305-ietf-decrypt-00942b3f84 failure_category=builtin_behavior requires_ref_extension=sodium
try {
    $result = \sodium_crypto_aead_xchacha20poly1305_ietf_decrypt([], "", "", "");
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
