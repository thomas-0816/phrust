<?php
// oracle-probe: id=oracle-builtin-contract-function-preg-match-30462015e0 area=builtin_contract kind=function symbol=preg_match source=ext/pcre/php_pcre.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-preg-match-30462015e0 failure_category=builtin_contract requires_ref_extension=pcre
try {
    $result = \preg_match();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
