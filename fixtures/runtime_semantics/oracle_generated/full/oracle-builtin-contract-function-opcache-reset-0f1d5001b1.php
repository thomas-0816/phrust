<?php
// oracle-probe: id=oracle-builtin-contract-function-opcache-reset-0f1d5001b1 area=builtin_contract kind=function symbol=opcache_reset source=ext/opcache/opcache.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-opcache-reset-0f1d5001b1 failure_category=builtin_contract requires_ref_extension=opcache
$name = "opcache_reset";
echo function_exists($name) ? "available\n" : "missing\n";
