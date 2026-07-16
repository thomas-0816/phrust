<?php
// oracle-probe: id=oracle-builtin-contract-function-opcache-is-script-cached-in-file-cache-55816610fd area=builtin_contract kind=function symbol=opcache_is_script_cached_in_file_cache source=ext/opcache/opcache.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-opcache-is-script-cached-in-file-cache-55816610fd failure_category=builtin_contract requires_ref_extension=opcache
$name = "opcache_is_script_cached_in_file_cache";
echo function_exists($name) ? "available\n" : "missing\n";
