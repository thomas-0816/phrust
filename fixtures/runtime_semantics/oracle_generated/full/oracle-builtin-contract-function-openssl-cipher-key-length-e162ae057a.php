<?php
// oracle-probe: id=oracle-builtin-contract-function-openssl-cipher-key-length-e162ae057a area=builtin_contract kind=function symbol=openssl_cipher_key_length source=ext/openssl/openssl.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-openssl-cipher-key-length-e162ae057a failure_category=builtin_contract requires_ref_extension=openssl
try {
    $result = \openssl_cipher_key_length();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
