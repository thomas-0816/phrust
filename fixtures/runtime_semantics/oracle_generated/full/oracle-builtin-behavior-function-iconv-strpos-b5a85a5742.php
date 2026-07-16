<?php
// oracle-probe: id=oracle-builtin-behavior-function-iconv-strpos-b5a85a5742 area=builtin_behavior kind=function symbol=iconv_strpos source=ext/iconv/iconv.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-behavior-function-iconv-strpos-b5a85a5742 failure_category=builtin_behavior requires_ref_extension=iconv
try {
    $result = \iconv_strpos([], "");
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
