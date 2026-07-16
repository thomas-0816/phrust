<?php
// oracle-probe: id=oracle-builtin-contract-function-sodium-crypto-box-seal-open-cfb86e94de area=builtin_contract kind=function symbol=sodium_crypto_box_seal_open source=ext/sodium/libsodium.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-sodium-crypto-box-seal-open-cfb86e94de failure_category=builtin_contract requires_ref_extension=sodium
$name = "sodium_crypto_box_seal_open";
echo function_exists($name) ? "available\n" : "missing\n";
