<?php
// oracle-probe: id=oracle-builtin-contract-function-mb-stripos-dac0f33c7d area=builtin_contract kind=function symbol=mb_stripos source=ext/mbstring/mbstring.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-mb-stripos-dac0f33c7d failure_category=builtin_contract requires_ref_extension=mbstring
$name = "mb_stripos";
echo function_exists($name) ? "available\n" : "missing\n";
