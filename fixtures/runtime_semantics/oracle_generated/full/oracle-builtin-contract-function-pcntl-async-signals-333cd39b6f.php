<?php
// oracle-probe: id=oracle-builtin-contract-function-pcntl-async-signals-333cd39b6f area=builtin_contract kind=function symbol=pcntl_async_signals source=ext/pcntl/pcntl.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-pcntl-async-signals-333cd39b6f failure_category=builtin_contract requires_ref_extension=pcntl
$name = "pcntl_async_signals";
echo function_exists($name) ? "available\n" : "missing\n";
