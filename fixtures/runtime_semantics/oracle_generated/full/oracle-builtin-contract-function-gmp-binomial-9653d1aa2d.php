<?php
// oracle-probe: id=oracle-builtin-contract-function-gmp-binomial-9653d1aa2d area=builtin_contract kind=function symbol=gmp_binomial source=ext/gmp/gmp.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-gmp-binomial-9653d1aa2d failure_category=builtin_contract requires_ref_extension=gmp
$name = "gmp_binomial";
echo function_exists($name) ? "available\n" : "missing\n";
