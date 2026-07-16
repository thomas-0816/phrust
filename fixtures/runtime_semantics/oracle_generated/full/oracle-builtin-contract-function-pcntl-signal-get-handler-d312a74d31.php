<?php
// oracle-probe: id=oracle-builtin-contract-function-pcntl-signal-get-handler-d312a74d31 area=builtin_contract kind=function symbol=pcntl_signal_get_handler source=ext/pcntl/pcntl.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-pcntl-signal-get-handler-d312a74d31 failure_category=builtin_contract requires_ref_extension=pcntl
$name = "pcntl_signal_get_handler";
echo function_exists($name) ? "available\n" : "missing\n";
