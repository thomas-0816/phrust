<?php
// oracle-probe: id=oracle-builtin-contract-function-gmp-popcount-ca776931f5 area=builtin_contract kind=function symbol=gmp_popcount source=ext/gmp/gmp.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-gmp-popcount-ca776931f5 failure_category=builtin_contract requires_ref_extension=gmp
$name = "gmp_popcount";
echo function_exists($name) ? "available\n" : "missing\n";
