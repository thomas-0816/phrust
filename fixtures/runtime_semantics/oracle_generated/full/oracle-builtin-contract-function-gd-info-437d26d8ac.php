<?php
// oracle-probe: id=oracle-builtin-contract-function-gd-info-437d26d8ac area=builtin_contract kind=function symbol=gd_info source=ext/gd/gd.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-gd-info-437d26d8ac failure_category=builtin_contract requires_ref_extension=gd
$name = "gd_info";
echo function_exists($name) ? "available\n" : "missing\n";
