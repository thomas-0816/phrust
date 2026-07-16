<?php
// oracle-probe: id=oracle-builtin-contract-function-openssl-x509-read-c6c66cd5e9 area=builtin_contract kind=function symbol=openssl_x509_read source=ext/openssl/openssl.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-openssl-x509-read-c6c66cd5e9 failure_category=builtin_contract requires_ref_extension=openssl
$name = "openssl_x509_read";
echo function_exists($name) ? "available\n" : "missing\n";
