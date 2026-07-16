<?php
// oracle-probe: id=oracle-builtin-contract-function-iconv-strpos-d46b39fc5f area=builtin_contract kind=function symbol=iconv_strpos source=ext/iconv/iconv.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-iconv-strpos-d46b39fc5f failure_category=builtin_contract requires_ref_extension=iconv
try {
    $result = \iconv_strpos();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
