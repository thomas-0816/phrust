<?php
// oracle-probe: id=oracle-builtin-behavior-function-sodium-crypto-generichash-keygen-7539bf15b8 area=builtin_behavior kind=function symbol=sodium_crypto_generichash_keygen source=ext/sodium/libsodium.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-behavior-function-sodium-crypto-generichash-keygen-7539bf15b8 failure_category=builtin_behavior requires_ref_extension=sodium
try {
    $result = \sodium_crypto_generichash_keygen();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
