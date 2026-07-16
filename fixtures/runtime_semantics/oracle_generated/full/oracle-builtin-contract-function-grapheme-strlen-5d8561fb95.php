<?php
// oracle-probe: id=oracle-builtin-contract-function-grapheme-strlen-5d8561fb95 area=builtin_contract kind=function symbol=grapheme_strlen source=ext/intl/php_intl.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-grapheme-strlen-5d8561fb95 failure_category=builtin_contract requires_ref_extension=intl
$name = "grapheme_strlen";
echo function_exists($name) ? "available\n" : "missing\n";
