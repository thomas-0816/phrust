<?php
// oracle-probe: id=oracle-builtin-contract-function-opcache-compile-file-bc4c265cad area=builtin_contract kind=function symbol=opcache_compile_file source=ext/opcache/opcache.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-opcache-compile-file-bc4c265cad failure_category=builtin_contract requires_ref_extension=opcache
$name = "opcache_compile_file";
echo function_exists($name) ? "available\n" : "missing\n";
