<?php
// oracle-probe: id=oracle-builtin-behavior-function-jdtojewish-8ab263f27f area=builtin_behavior kind=function symbol=jdtojewish source=ext/calendar/calendar.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-behavior-function-jdtojewish-8ab263f27f failure_category=builtin_behavior requires_ref_extension=calendar
try {
    $result = \jdtojewish(0, false);
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
