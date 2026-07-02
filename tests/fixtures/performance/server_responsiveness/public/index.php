<?php
$path = $_SERVER["REQUEST_URI"] ?? "/";
echo "front:", strlen($path), "\n";
