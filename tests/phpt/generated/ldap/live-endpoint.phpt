--TEST--
ldap live endpoint smoke through ldap3 backend
--SKIPIF--
<?php
if (!extension_loaded("ldap")) die("skip ldap extension not loaded");
if (getenv("PHRUST_NET_TESTS") !== "1") die("skip PHRUST_NET_TESTS=1 not set");
if (getenv("PHRUST_LDAP_LIVE_URI") === false) die("skip PHRUST_LDAP_LIVE_URI not set");
if (getenv("PHRUST_LDAP_LIVE_BASE_DN") === false) die("skip PHRUST_LDAP_LIVE_BASE_DN not set");
?>
--FILE--
<?php
$uri = getenv("PHRUST_LDAP_LIVE_URI");
$bind_dn = getenv("PHRUST_LDAP_LIVE_BIND_DN");
$password = getenv("PHRUST_LDAP_LIVE_PASSWORD");
$base = getenv("PHRUST_LDAP_LIVE_BASE_DN");
$filter = getenv("PHRUST_LDAP_LIVE_FILTER") ?: "(objectClass=*)";

$ldap = ldap_connect($uri);
var_dump($ldap instanceof LDAP\Connection);

var_dump(ldap_bind($ldap, $bind_dn ?: null, $password ?: null));
var_dump(ldap_errno($ldap));

$result = ldap_search($ldap, $base, $filter, ["dn"]);
var_dump($result instanceof LDAP\Result);
var_dump(is_int(ldap_count_entries($ldap, $result)));

$entries = ldap_get_entries($ldap, $result);
var_dump(is_array($entries));
var_dump(array_key_exists("count", $entries));

$entry = ldap_first_entry($ldap, $result);
var_dump($entry === false || $entry instanceof LDAP\ResultEntry);
if ($entry !== false) {
    var_dump(is_array(ldap_get_attributes($ldap, $entry)));
    $next = ldap_next_entry($ldap, $entry);
    var_dump($next === false || $next instanceof LDAP\ResultEntry);
} else {
    var_dump(true);
    var_dump(true);
}

var_dump(ldap_unbind($ldap));
?>
--EXPECT--
bool(true)
bool(true)
int(0)
bool(true)
bool(true)
bool(true)
bool(true)
bool(true)
bool(true)
bool(true)
bool(true)
