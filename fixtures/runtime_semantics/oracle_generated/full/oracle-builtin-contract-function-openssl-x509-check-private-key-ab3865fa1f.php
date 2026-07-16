<?php
// oracle-probe: id=oracle-builtin-contract-function-openssl-x509-check-private-key-ab3865fa1f area=builtin_contract kind=function symbol=openssl_x509_check_private_key source=ext/openssl/openssl.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-openssl-x509-check-private-key-ab3865fa1f failure_category=builtin_contract requires_ref_extension=openssl
$name = "openssl_x509_check_private_key";
echo function_exists($name) ? "available\n" : "missing\n";
