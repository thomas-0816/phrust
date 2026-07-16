<?php
// oracle-probe: id=oracle-builtin-contract-function-openssl-get-privatekey-de670552af area=builtin_contract kind=function symbol=openssl_get_privatekey source=ext/openssl/openssl.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-openssl-get-privatekey-de670552af failure_category=builtin_contract requires_ref_extension=openssl
$name = "openssl_get_privatekey";
echo function_exists($name) ? "available\n" : "missing\n";
