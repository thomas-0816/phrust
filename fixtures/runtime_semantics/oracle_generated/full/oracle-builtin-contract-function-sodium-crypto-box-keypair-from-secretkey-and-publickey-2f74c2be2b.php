<?php
// oracle-probe: id=oracle-builtin-contract-function-sodium-crypto-box-keypair-from-secretkey-and-publickey-2f74c2be2b area=builtin_contract kind=function symbol=sodium_crypto_box_keypair_from_secretkey_and_publickey source=ext/sodium/libsodium.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-sodium-crypto-box-keypair-from-secretkey-and-publickey-2f74c2be2b failure_category=builtin_contract requires_ref_extension=sodium
$name = "sodium_crypto_box_keypair_from_secretkey_and_publickey";
echo function_exists($name) ? "available\n" : "missing\n";
