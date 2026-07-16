<?php
// oracle-probe: id=oracle-builtin-contract-function-openssl-cipher-key-length-fab5fae9ec area=builtin_contract kind=function symbol=openssl_cipher_key_length source=ext/openssl/openssl.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-openssl-cipher-key-length-fab5fae9ec failure_category=builtin_contract requires_ref_extension=openssl
$name = "openssl_cipher_key_length";
echo function_exists($name) ? "available\n" : "missing\n";
