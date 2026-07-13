--TEST--
memcached opt-in live endpoint core commands
--SKIPIF--
<?php
if (!extension_loaded("memcached")) die("skip memcached extension not loaded");
if (!getenv("PHRUST_MEMCACHED_LIVE_ENDPOINT")) die("skip PHRUST_MEMCACHED_LIVE_ENDPOINT not set");
?>
--FILE--
<?php
$endpoint = getenv("PHRUST_MEMCACHED_LIVE_ENDPOINT");
$parts = explode(":", $endpoint, 2);
$host = $parts[0];
$port = isset($parts[1]) ? (int) $parts[1] : 11211;
$m = new Memcached();
var_dump($m->addServer($host, $port));
$prefix = "phrust:memcached:live:";
$m->delete($prefix . "alpha");
$m->delete($prefix . "beta");
$m->delete($prefix . "gamma");
$m->delete($prefix . "count");
var_dump($m->set($prefix . "alpha", "one"));
var_dump($m->get($prefix . "alpha"));
var_dump($m->setMulti([$prefix . "beta" => "two", $prefix . "gamma" => "three"]));
$many = $m->getMulti([$prefix . "alpha", $prefix . "beta", $prefix . "missing"]);
var_dump($many[$prefix . "alpha"], $many[$prefix . "beta"], isset($many[$prefix . "missing"]));
var_dump($m->increment($prefix . "count", 2, 10));
var_dump($m->decrement($prefix . "count", 3));
var_dump($m->delete($prefix . "alpha"));
var_dump($m->get($prefix . "alpha"));
var_dump($m->getResultCode());
$m->delete($prefix . "beta");
$m->delete($prefix . "gamma");
$m->delete($prefix . "count");
?>
--EXPECT--
bool(true)
bool(true)
string(3) "one"
bool(true)
string(3) "one"
string(3) "two"
bool(false)
int(10)
int(7)
bool(true)
bool(false)
int(16)
