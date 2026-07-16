<?php
// oracle-probe: id=oracle-builtin-contract-function-iconv-strrpos-6a297d9f73 area=builtin_contract kind=function symbol=iconv_strrpos source=ext/iconv/iconv.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-iconv-strrpos-6a297d9f73 failure_category=builtin_contract requires_ref_extension=iconv
$name = "iconv_strrpos";
echo function_exists($name) ? "available\n" : "missing\n";
