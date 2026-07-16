<?php
// oracle-probe: id=oracle-builtin-behavior-function-iconv-mime-decode-a33c5fe0ae area=builtin_behavior kind=function symbol=iconv_mime_decode source=ext/iconv/iconv.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-behavior-function-iconv-mime-decode-a33c5fe0ae failure_category=builtin_behavior requires_ref_extension=iconv
try {
    $result = \iconv_mime_decode("", 0);
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
