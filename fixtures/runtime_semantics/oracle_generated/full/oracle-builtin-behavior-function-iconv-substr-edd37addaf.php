<?php
// oracle-probe: id=oracle-builtin-behavior-function-iconv-substr-edd37addaf area=builtin_behavior kind=function symbol=iconv_substr source=ext/iconv/iconv.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-behavior-function-iconv-substr-edd37addaf failure_category=builtin_behavior requires_ref_extension=iconv
try {
    $result = \iconv_substr("", 0);
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
