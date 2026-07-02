<?php
// oracle-probe: id=oracle-frontend-lowering-destructuring-nested-keyed-ee572bed9b area=frontend_lowering kind=destructuring symbol=nested-keyed source=seed expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-frontend-lowering-destructuring-nested-keyed-ee572bed9b failure_category=frontend_lowering
$row = ["a" => [1, 2], "b" => 3]; ["a" => [$x, $y], "b" => $z] = $row; echo $x, ":", $y, ":", $z, "\n";
