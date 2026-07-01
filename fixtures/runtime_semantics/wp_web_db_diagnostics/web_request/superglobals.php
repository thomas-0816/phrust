<?php
echo $_SERVER['REQUEST_METHOD'] ?? '';
echo "\n";
echo $_SERVER['REQUEST_URI'] ?? '';
echo "\n";
echo $_SERVER['PATH_INFO'] ?? '';
echo "\n";
echo $_GET['alpha'] ?? '';
echo "\n";
echo $_POST['beta'] ?? '';
echo "\n";
echo $_COOKIE['session'] ?? '';
echo "\n";
echo $_SERVER['HTTP_X_WP_FIXTURE'] ?? '';
