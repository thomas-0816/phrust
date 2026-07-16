<?php
// oracle-probe: id=oracle-builtin-contract-function-pcntl-fork-1c8c06968e area=builtin_contract kind=function symbol=pcntl_fork source=ext/pcntl/pcntl.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-pcntl-fork-1c8c06968e failure_category=builtin_contract requires_ref_extension=pcntl
$name = "pcntl_fork";
echo function_exists($name) ? "available\n" : "missing\n";
