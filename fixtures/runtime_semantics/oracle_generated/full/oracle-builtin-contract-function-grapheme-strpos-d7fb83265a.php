<?php
// oracle-probe: id=oracle-builtin-contract-function-grapheme-strpos-d7fb83265a area=builtin_contract kind=function symbol=grapheme_strpos source=ext/intl/php_intl.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-grapheme-strpos-d7fb83265a failure_category=builtin_contract requires_ref_extension=intl
$name = "grapheme_strpos";
echo function_exists($name) ? "available\n" : "missing\n";
