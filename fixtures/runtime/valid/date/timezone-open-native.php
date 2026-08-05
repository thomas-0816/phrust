<?php
$timezone = timezone_open("Europe/Berlin");
echo get_class($timezone), "\n";
var_dump(timezone_open("Not/A_Timezone"));
