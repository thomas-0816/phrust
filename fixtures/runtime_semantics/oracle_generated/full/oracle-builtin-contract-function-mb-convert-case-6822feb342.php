<?php
// oracle-probe: id=oracle-builtin-contract-function-mb-convert-case-6822feb342 area=builtin_contract kind=function symbol=mb_convert_case source=ext/mbstring/mbstring.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-mb-convert-case-6822feb342 failure_category=builtin_contract requires_ref_extension=mbstring
$name = "mb_convert_case";
echo function_exists($name) ? "available\n" : "missing\n";
