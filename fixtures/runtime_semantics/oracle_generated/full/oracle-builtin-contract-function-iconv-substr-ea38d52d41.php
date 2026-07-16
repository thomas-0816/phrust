<?php
// oracle-probe: id=oracle-builtin-contract-function-iconv-substr-ea38d52d41 area=builtin_contract kind=function symbol=iconv_substr source=ext/iconv/iconv.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-iconv-substr-ea38d52d41 failure_category=builtin_contract requires_ref_extension=iconv
try {
    $result = \iconv_substr();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
