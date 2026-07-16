<?php
// oracle-probe: id=oracle-builtin-contract-function-openssl-x509-parse-4002122fa2 area=builtin_contract kind=function symbol=openssl_x509_parse source=ext/openssl/openssl.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-openssl-x509-parse-4002122fa2 failure_category=builtin_contract requires_ref_extension=openssl
$name = "openssl_x509_parse";
echo function_exists($name) ? "available\n" : "missing\n";
