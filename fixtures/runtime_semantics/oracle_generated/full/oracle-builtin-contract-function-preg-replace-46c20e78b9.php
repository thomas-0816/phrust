<?php
// oracle-probe: id=oracle-builtin-contract-function-preg-replace-46c20e78b9 area=builtin_contract kind=function symbol=preg_replace source=ext/pcre/php_pcre.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-preg-replace-46c20e78b9 failure_category=builtin_contract requires_ref_extension=pcre
try {
    $result = \preg_replace();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
