<?php
// oracle-probe: id=oracle-builtin-contract-function-iconv-strpos-7c94443965 area=builtin_contract kind=function symbol=iconv_strpos source=ext/iconv/iconv.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-iconv-strpos-7c94443965 failure_category=builtin_contract requires_ref_extension=iconv
$name = "iconv_strpos";
echo function_exists($name) ? "available\n" : "missing\n";
