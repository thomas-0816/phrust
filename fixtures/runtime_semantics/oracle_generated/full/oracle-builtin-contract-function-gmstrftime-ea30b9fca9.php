<?php
// oracle-probe: id=oracle-builtin-contract-function-gmstrftime-ea30b9fca9 area=builtin_contract kind=function symbol=gmstrftime source=ext/date/php_date.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-gmstrftime-ea30b9fca9 failure_category=builtin_contract requires_ref_extension=date
$name = "gmstrftime";
echo function_exists($name) ? "available\n" : "missing\n";
