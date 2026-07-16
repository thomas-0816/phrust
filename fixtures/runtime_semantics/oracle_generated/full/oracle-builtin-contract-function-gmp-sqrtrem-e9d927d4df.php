<?php
// oracle-probe: id=oracle-builtin-contract-function-gmp-sqrtrem-e9d927d4df area=builtin_contract kind=function symbol=gmp_sqrtrem source=ext/gmp/gmp.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-gmp-sqrtrem-e9d927d4df failure_category=builtin_contract requires_ref_extension=gmp
$name = "gmp_sqrtrem";
echo function_exists($name) ? "available\n" : "missing\n";
