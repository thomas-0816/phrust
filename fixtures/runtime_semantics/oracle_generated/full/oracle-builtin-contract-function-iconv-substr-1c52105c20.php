<?php
// oracle-probe: id=oracle-builtin-contract-function-iconv-substr-1c52105c20 area=builtin_contract kind=function symbol=iconv_substr source=ext/iconv/iconv.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-iconv-substr-1c52105c20 failure_category=builtin_contract requires_ref_extension=iconv
$name = "iconv_substr";
echo function_exists($name) ? "available\n" : "missing\n";
