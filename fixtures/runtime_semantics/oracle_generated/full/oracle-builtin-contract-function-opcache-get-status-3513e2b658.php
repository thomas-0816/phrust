<?php
// oracle-probe: id=oracle-builtin-contract-function-opcache-get-status-3513e2b658 area=builtin_contract kind=function symbol=opcache_get_status source=ext/opcache/opcache.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-opcache-get-status-3513e2b658 failure_category=builtin_contract requires_ref_extension=opcache
$name = "opcache_get_status";
echo function_exists($name) ? "available\n" : "missing\n";
