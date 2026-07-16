<?php
// oracle-probe: id=oracle-builtin-contract-function-pcntl-wtermsig-b8440063ae area=builtin_contract kind=function symbol=pcntl_wtermsig source=ext/pcntl/pcntl.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-pcntl-wtermsig-b8440063ae failure_category=builtin_contract requires_ref_extension=pcntl
$name = "pcntl_wtermsig";
echo function_exists($name) ? "available\n" : "missing\n";
