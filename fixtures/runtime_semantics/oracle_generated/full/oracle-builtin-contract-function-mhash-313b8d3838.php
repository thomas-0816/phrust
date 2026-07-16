<?php
// oracle-probe: id=oracle-builtin-contract-function-mhash-313b8d3838 area=builtin_contract kind=function symbol=mhash source=ext/hash/hash.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-mhash-313b8d3838 failure_category=builtin_contract requires_ref_extension=hash
$name = "mhash";
echo function_exists($name) ? "available\n" : "missing\n";
