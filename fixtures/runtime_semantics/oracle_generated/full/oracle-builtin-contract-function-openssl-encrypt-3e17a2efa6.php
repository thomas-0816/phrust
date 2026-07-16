<?php
// oracle-probe: id=oracle-builtin-contract-function-openssl-encrypt-3e17a2efa6 area=builtin_contract kind=function symbol=openssl_encrypt source=ext/openssl/openssl.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-openssl-encrypt-3e17a2efa6 failure_category=builtin_contract requires_ref_extension=openssl
$name = "openssl_encrypt";
echo function_exists($name) ? "available\n" : "missing\n";
