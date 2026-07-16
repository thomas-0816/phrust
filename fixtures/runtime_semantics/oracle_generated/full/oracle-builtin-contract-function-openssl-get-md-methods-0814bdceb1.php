<?php
// oracle-probe: id=oracle-builtin-contract-function-openssl-get-md-methods-0814bdceb1 area=builtin_contract kind=function symbol=openssl_get_md_methods source=ext/openssl/openssl.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-openssl-get-md-methods-0814bdceb1 failure_category=builtin_contract requires_ref_extension=openssl
try {
    $result = \openssl_get_md_methods(__phrust_probe_unknown: 1);
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
