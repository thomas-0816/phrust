<?php
// oracle-probe: id=oracle-builtin-behavior-function-mb-parse-str-f274a1e839 area=builtin_behavior kind=function symbol=mb_parse_str source=ext/mbstring/mbstring.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-behavior-function-mb-parse-str-f274a1e839 failure_category=builtin_behavior requires_ref_extension=mbstring
$arg1 = null;
try {
    $result = \mb_parse_str([], $arg1);
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
