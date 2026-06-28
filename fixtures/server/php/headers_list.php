<?php
header("X-Test: yes");
$headers = headers_list();
echo $headers[0], "\n";
