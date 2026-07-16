<?php
// oracle-probe: id=oracle-builtin-contract-function-gmp-rootrem-eb47a8c351 area=builtin_contract kind=function symbol=gmp_rootrem source=ext/gmp/gmp.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-gmp-rootrem-eb47a8c351 failure_category=builtin_contract requires_ref_extension=gmp
$name = "gmp_rootrem";
echo function_exists($name) ? "available\n" : "missing\n";
