<?php

echo "root|";

$started = ob_start();
echo "outer|";
$outerContents = ob_get_contents();
$outerLength = ob_get_length();
$outerLevel = ob_get_level();

$nestedStarted = ob_start();
echo "inner";
$innerContents = ob_get_contents();
$innerLength = ob_get_length();
$innerLevel = ob_get_level();
$innerFlushed = ob_get_flush();
$afterInnerFlush = ob_get_contents();
$afterInnerLevel = ob_get_level();
$outerCleaned = ob_get_clean();

$flushStarted = ob_start();
echo "flushme|";
$flushContents = ob_get_flush();

$endFlushStarted = ob_start();
echo "endflush|";
$endFlushed = ob_end_flush();

$endCleanStarted = ob_start();
echo "discarded";
$endCleaned = ob_end_clean();

$noContents = ob_get_contents();
$noLength = ob_get_length();
$noClean = ob_get_clean();
$previousErrors = error_reporting(0);
$noGetFlush = ob_get_flush();
$noEndFlush = ob_end_flush();
$noEndClean = ob_end_clean();
error_reporting($previousErrors);

echo "\nstarted=" . (int) $started;
echo "\nouter_contents=" . bin2hex($outerContents);
echo "\nouter_length=" . $outerLength;
echo "\nouter_level=" . $outerLevel;
echo "\nnested_started=" . (int) $nestedStarted;
echo "\ninner_contents=" . bin2hex($innerContents);
echo "\ninner_length=" . $innerLength;
echo "\ninner_level=" . $innerLevel;
echo "\ninner_flushed=" . bin2hex($innerFlushed);
echo "\nafter_inner_flush=" . bin2hex($afterInnerFlush);
echo "\nafter_inner_level=" . $afterInnerLevel;
echo "\nouter_cleaned=" . bin2hex($outerCleaned);
echo "\nflush_started=" . (int) $flushStarted;
echo "\nflush_contents=" . bin2hex($flushContents);
echo "\nend_flush_started=" . (int) $endFlushStarted;
echo "\nend_flushed=" . (int) $endFlushed;
echo "\nend_clean_started=" . (int) $endCleanStarted;
echo "\nend_cleaned=" . (int) $endCleaned;
echo "\nno_contents=" . (int) $noContents;
echo "\nno_length=" . (int) $noLength;
echo "\nno_clean=" . (int) $noClean;
echo "\nno_get_flush=" . (int) $noGetFlush;
echo "\nno_end_flush=" . (int) $noEndFlush;
echo "\nno_end_clean=" . (int) $noEndClean;
echo "\nfinal_level=" . ob_get_level() . "\n";
