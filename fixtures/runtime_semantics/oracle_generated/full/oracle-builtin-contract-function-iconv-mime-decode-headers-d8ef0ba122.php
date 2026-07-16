<?php
// oracle-probe: id=oracle-builtin-contract-function-iconv-mime-decode-headers-d8ef0ba122 area=builtin_contract kind=function symbol=iconv_mime_decode_headers source=ext/iconv/iconv.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-iconv-mime-decode-headers-d8ef0ba122 failure_category=builtin_contract requires_ref_extension=iconv
$name = "iconv_mime_decode_headers";
echo function_exists($name) ? "available\n" : "missing\n";
