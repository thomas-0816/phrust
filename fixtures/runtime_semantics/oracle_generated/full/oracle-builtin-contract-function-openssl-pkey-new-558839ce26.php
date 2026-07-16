<?php
// oracle-probe: id=oracle-builtin-contract-function-openssl-pkey-new-558839ce26 area=builtin_contract kind=function symbol=openssl_pkey_new source=ext/openssl/openssl.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-openssl-pkey-new-558839ce26 failure_category=builtin_contract requires_ref_extension=openssl
$name = "openssl_pkey_new";
echo function_exists($name) ? "available\n" : "missing\n";
