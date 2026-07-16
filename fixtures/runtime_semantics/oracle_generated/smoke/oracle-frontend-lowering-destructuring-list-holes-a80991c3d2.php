<?php
// oracle-probe: id=oracle-frontend-lowering-destructuring-list-holes-a80991c3d2 area=frontend_lowering kind=destructuring symbol=list-holes source=seed expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-frontend-lowering-destructuring-list-holes-a80991c3d2 failure_category=frontend_lowering
[$first, , $third] = [1, 2, 3]; echo $first, ":", $third, "\n";
