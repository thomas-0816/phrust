<?php
// oracle-probe: id=oracle-builtin-contract-function-stream-wrapper-register-b109f6ed0d area=builtin_contract kind=function symbol=stream_wrapper_register source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-stream-wrapper-register-b109f6ed0d failure_category=builtin_contract
$name = "stream_wrapper_register";
echo function_exists($name) ? "available\n" : "missing\n";
