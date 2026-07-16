<?php
// oracle-probe: id=oracle-builtin-contract-function-openssl-pkey-new-02c3f06f77 area=builtin_contract kind=function symbol=openssl_pkey_new source=ext/openssl/openssl.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-openssl-pkey-new-02c3f06f77 failure_category=builtin_contract requires_ref_extension=openssl
try {
    $result = \openssl_pkey_new(__phrust_probe_unknown: 1);
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
