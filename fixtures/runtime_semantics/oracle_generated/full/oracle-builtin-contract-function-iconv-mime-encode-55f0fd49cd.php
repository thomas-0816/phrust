<?php
// oracle-probe: id=oracle-builtin-contract-function-iconv-mime-encode-55f0fd49cd area=builtin_contract kind=function symbol=iconv_mime_encode source=ext/iconv/iconv.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-iconv-mime-encode-55f0fd49cd failure_category=builtin_contract requires_ref_extension=iconv
$name = "iconv_mime_encode";
echo function_exists($name) ? "available\n" : "missing\n";
