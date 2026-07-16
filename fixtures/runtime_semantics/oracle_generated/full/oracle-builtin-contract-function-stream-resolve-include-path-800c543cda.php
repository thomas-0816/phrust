<?php
// oracle-probe: id=oracle-builtin-contract-function-stream-resolve-include-path-800c543cda area=builtin_contract kind=function symbol=stream_resolve_include_path source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-stream-resolve-include-path-800c543cda failure_category=builtin_contract
$name = "stream_resolve_include_path";
echo function_exists($name) ? "available\n" : "missing\n";
