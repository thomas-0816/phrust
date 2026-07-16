<?php
// oracle-probe: id=oracle-builtin-behavior-function-preg-match-c5a4fb35b4 area=builtin_behavior kind=function symbol=preg_match source=ext/pcre/php_pcre.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-behavior-function-preg-match-c5a4fb35b4 failure_category=builtin_behavior requires_ref_extension=pcre
$arg2 = null;
try {
    $result = \preg_match("", "", $arg2);
    echo "return:\n";
    var_dump($result);
    echo "writeback:\n";
    var_dump($arg2);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
