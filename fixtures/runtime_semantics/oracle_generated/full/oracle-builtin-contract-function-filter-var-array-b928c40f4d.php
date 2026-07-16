<?php
// oracle-probe: id=oracle-builtin-contract-function-filter-var-array-b928c40f4d area=builtin_contract kind=function symbol=filter_var_array source=ext/filter/filter.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-filter-var-array-b928c40f4d failure_category=builtin_contract requires_ref_extension=filter
$name = "filter_var_array";
echo function_exists($name) ? "available\n" : "missing\n";
