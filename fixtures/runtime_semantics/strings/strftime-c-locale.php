<?php

error_reporting(2047);
date_default_timezone_set('UTC');
$timestamp = strtotime('10:00:00 AM July 1 2005');
echo strftime('%r %B%e %Y %Z %z', $timestamp), "\n";
echo gmstrftime('%F %T %Z %z', $timestamp), "\n";

date_default_timezone_set('Australia/Sydney');
$timestamp = strtotime('10:00:00 AM July 1 2005');
echo strftime('%r %B%e %Y %Z %z', $timestamp), "\n";
