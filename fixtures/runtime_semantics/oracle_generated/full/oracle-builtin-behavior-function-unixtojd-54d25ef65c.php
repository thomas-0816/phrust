<?php
// oracle-probe: id=oracle-builtin-behavior-function-unixtojd-54d25ef65c area=builtin_behavior kind=function symbol=unixtojd source=ext/calendar/calendar.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-behavior-function-unixtojd-54d25ef65c failure_category=builtin_behavior requires_ref_extension=calendar
try {
    $result = \unixtojd();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
