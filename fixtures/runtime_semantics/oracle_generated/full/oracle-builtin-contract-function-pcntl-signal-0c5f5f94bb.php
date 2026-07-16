<?php
// oracle-probe: id=oracle-builtin-contract-function-pcntl-signal-0c5f5f94bb area=builtin_contract kind=function symbol=pcntl_signal source=ext/pcntl/pcntl.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-pcntl-signal-0c5f5f94bb failure_category=builtin_contract requires_ref_extension=pcntl
$name = "pcntl_signal";
echo function_exists($name) ? "available\n" : "missing\n";
