<?php

$zone = new DateTimeZone('UTC');
$immutable = new DateTimeImmutable('2026-07-15 13:35:20', $zone);

echo $immutable->format('Y-m-d H:i:s e'), "\n";
echo $immutable->getTimestamp(), "\n";
echo $immutable->getTimezone()->getName(), "\n";

$changed = $immutable->add(new DateInterval('P1DT2H3M4S'));
echo $changed->format('Y-m-d H:i:s'), "\n";
echo $immutable->format('Y-m-d H:i:s'), "\n";

$mutable = new DateTime('2026-07-15 13:35:20', $zone);
$same = $mutable->add(new DateInterval('PT1H'));
var_dump($same === $mutable);
echo $mutable->format('Y-m-d H:i:s'), "\n";

$modified = $mutable->modify('+1 day');
var_dump($modified === $mutable);
echo $mutable->format('Y-m-d H:i:s'), "\n";

$diff = $immutable->diff($changed);
echo $diff->format('%R%a %H:%I:%S'), "\n";
