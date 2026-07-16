<?php
// oracle-probe: id=oracle-builtin-contract-function-pcntl-wait-c1a57b1517 area=builtin_contract kind=function symbol=pcntl_wait source=ext/pcntl/pcntl.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-pcntl-wait-c1a57b1517 failure_category=builtin_contract requires_ref_extension=pcntl
$name = "pcntl_wait";
echo function_exists($name) ? "available\n" : "missing\n";
