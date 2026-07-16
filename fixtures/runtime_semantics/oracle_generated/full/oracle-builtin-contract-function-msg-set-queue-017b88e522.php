<?php
// oracle-probe: id=oracle-builtin-contract-function-msg-set-queue-017b88e522 area=builtin_contract kind=function symbol=msg_set_queue source=ext/sysvmsg/sysvmsg.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-msg-set-queue-017b88e522 failure_category=builtin_contract requires_ref_extension=sysvmsg
$name = "msg_set_queue";
echo function_exists($name) ? "available\n" : "missing\n";
