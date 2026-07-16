<?php
// oracle-probe: id=oracle-builtin-contract-function-mb-lcfirst-13bab5113b area=builtin_contract kind=function symbol=mb_lcfirst source=ext/mbstring/mbstring.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-mb-lcfirst-13bab5113b failure_category=builtin_contract requires_ref_extension=mbstring
$name = "mb_lcfirst";
echo function_exists($name) ? "available\n" : "missing\n";
