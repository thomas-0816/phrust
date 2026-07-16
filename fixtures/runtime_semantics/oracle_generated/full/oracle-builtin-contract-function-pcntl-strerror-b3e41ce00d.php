<?php
// oracle-probe: id=oracle-builtin-contract-function-pcntl-strerror-b3e41ce00d area=builtin_contract kind=function symbol=pcntl_strerror source=ext/pcntl/pcntl.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-pcntl-strerror-b3e41ce00d failure_category=builtin_contract requires_ref_extension=pcntl
$name = "pcntl_strerror";
echo function_exists($name) ? "available\n" : "missing\n";
