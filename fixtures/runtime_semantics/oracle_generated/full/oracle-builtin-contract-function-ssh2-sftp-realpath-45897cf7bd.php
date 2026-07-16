<?php
// oracle-probe: id=oracle-builtin-contract-function-ssh2-sftp-realpath-45897cf7bd area=builtin_contract kind=function symbol=ssh2_sftp_realpath source=Rust extension registry expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-ssh2-sftp-realpath-45897cf7bd failure_category=builtin_contract requires_ref_extension=ssh2
try {
    $result = \ssh2_sftp_realpath(__phrust_probe_unknown: 1);
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
