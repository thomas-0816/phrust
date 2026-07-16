<?php
// oracle-probe: id=oracle-builtin-contract-function-openssl-random-pseudo-bytes-8f4352a721 area=builtin_contract kind=function symbol=openssl_random_pseudo_bytes source=ext/openssl/openssl.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-openssl-random-pseudo-bytes-8f4352a721 failure_category=builtin_contract requires_ref_extension=openssl
try {
    $result = \openssl_random_pseudo_bytes();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
