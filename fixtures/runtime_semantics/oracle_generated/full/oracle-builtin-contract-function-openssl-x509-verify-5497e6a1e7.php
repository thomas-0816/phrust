<?php
// oracle-probe: id=oracle-builtin-contract-function-openssl-x509-verify-5497e6a1e7 area=builtin_contract kind=function symbol=openssl_x509_verify source=ext/openssl/openssl.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-openssl-x509-verify-5497e6a1e7 failure_category=builtin_contract requires_ref_extension=openssl
try {
    $result = \openssl_x509_verify();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
