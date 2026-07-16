<?php
// oracle-probe: id=oracle-builtin-contract-function-sodium-bin2base64-d2f488ed18 area=builtin_contract kind=function symbol=sodium_bin2base64 source=ext/sodium/libsodium.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-sodium-bin2base64-d2f488ed18 failure_category=builtin_contract requires_ref_extension=sodium
$name = "sodium_bin2base64";
echo function_exists($name) ? "available\n" : "missing\n";
