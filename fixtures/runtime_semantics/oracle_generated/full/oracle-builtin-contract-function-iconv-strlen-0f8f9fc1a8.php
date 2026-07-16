<?php
// oracle-probe: id=oracle-builtin-contract-function-iconv-strlen-0f8f9fc1a8 area=builtin_contract kind=function symbol=iconv_strlen source=ext/iconv/iconv.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-iconv-strlen-0f8f9fc1a8 failure_category=builtin_contract requires_ref_extension=iconv
$name = "iconv_strlen";
echo function_exists($name) ? "available\n" : "missing\n";
