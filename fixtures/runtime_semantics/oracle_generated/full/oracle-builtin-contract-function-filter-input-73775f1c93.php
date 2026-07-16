<?php
// oracle-probe: id=oracle-builtin-contract-function-filter-input-73775f1c93 area=builtin_contract kind=function symbol=filter_input source=ext/filter/filter.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-filter-input-73775f1c93 failure_category=builtin_contract requires_ref_extension=filter
$name = "filter_input";
echo function_exists($name) ? "available\n" : "missing\n";
