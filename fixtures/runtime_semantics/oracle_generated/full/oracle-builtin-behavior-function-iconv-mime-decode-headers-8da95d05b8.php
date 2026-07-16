<?php
// oracle-probe: id=oracle-builtin-behavior-function-iconv-mime-decode-headers-8da95d05b8 area=builtin_behavior kind=function symbol=iconv_mime_decode_headers source=ext/iconv/iconv.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-behavior-function-iconv-mime-decode-headers-8da95d05b8 failure_category=builtin_behavior requires_ref_extension=iconv
try {
    $result = \iconv_mime_decode_headers(headers: "");
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
