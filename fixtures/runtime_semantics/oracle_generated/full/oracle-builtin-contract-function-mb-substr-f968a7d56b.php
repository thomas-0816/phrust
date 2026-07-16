<?php
// oracle-probe: id=oracle-builtin-contract-function-mb-substr-f968a7d56b area=builtin_contract kind=function symbol=mb_substr source=ext/mbstring/mbstring.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-mb-substr-f968a7d56b failure_category=builtin_contract requires_ref_extension=mbstring
$name = "mb_substr";
echo function_exists($name) ? "available\n" : "missing\n";
