<?php
// oracle-probe: id=oracle-builtin-contract-function-filter-has-var-7ea7aa5893 area=builtin_contract kind=function symbol=filter_has_var source=ext/filter/filter.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-filter-has-var-7ea7aa5893 failure_category=builtin_contract requires_ref_extension=filter
$name = "filter_has_var";
echo function_exists($name) ? "available\n" : "missing\n";
