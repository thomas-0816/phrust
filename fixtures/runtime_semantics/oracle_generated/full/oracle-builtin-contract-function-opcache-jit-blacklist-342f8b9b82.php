<?php
// oracle-probe: id=oracle-builtin-contract-function-opcache-jit-blacklist-342f8b9b82 area=builtin_contract kind=function symbol=opcache_jit_blacklist source=ext/opcache/opcache.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-opcache-jit-blacklist-342f8b9b82 failure_category=builtin_contract requires_ref_extension=opcache
$name = "opcache_jit_blacklist";
echo function_exists($name) ? "available\n" : "missing\n";
