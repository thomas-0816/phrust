<?php
// oracle-probe: id=oracle-builtin-contract-function-normalizer-normalize-d6811f6fd4 area=builtin_contract kind=function symbol=normalizer_normalize source=ext/intl/php_intl.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-normalizer-normalize-d6811f6fd4 failure_category=builtin_contract requires_ref_extension=intl
$name = "normalizer_normalize";
echo function_exists($name) ? "available\n" : "missing\n";
