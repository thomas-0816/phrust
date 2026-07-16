<?php
// oracle-probe: id=oracle-builtin-contract-function-normalizer-is-normalized-a9e65f378d area=builtin_contract kind=function symbol=normalizer_is_normalized source=ext/intl/php_intl.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-normalizer-is-normalized-a9e65f378d failure_category=builtin_contract requires_ref_extension=intl
$name = "normalizer_is_normalized";
echo function_exists($name) ? "available\n" : "missing\n";
