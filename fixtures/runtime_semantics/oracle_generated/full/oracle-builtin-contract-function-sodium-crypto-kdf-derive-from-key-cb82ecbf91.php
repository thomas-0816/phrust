<?php
// oracle-probe: id=oracle-builtin-contract-function-sodium-crypto-kdf-derive-from-key-cb82ecbf91 area=builtin_contract kind=function symbol=sodium_crypto_kdf_derive_from_key source=ext/sodium/libsodium.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-sodium-crypto-kdf-derive-from-key-cb82ecbf91 failure_category=builtin_contract requires_ref_extension=sodium
try {
    $result = \sodium_crypto_kdf_derive_from_key();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
