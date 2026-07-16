<?php
// oracle-probe: id=oracle-builtin-contract-function-preg-split-1cc6b5c1b1 area=builtin_contract kind=function symbol=preg_split source=ext/pcre/php_pcre.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-preg-split-1cc6b5c1b1 failure_category=builtin_contract requires_ref_extension=pcre
try {
    $result = \preg_split();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
