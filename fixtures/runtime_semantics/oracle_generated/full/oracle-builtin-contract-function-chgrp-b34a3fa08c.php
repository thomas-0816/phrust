<?php
// oracle-probe: id=oracle-builtin-contract-function-chgrp-b34a3fa08c area=builtin_contract kind=function symbol=chgrp source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-chgrp-b34a3fa08c failure_category=builtin_contract
$name = "chgrp";
echo function_exists($name) ? "available\n" : "missing\n";
