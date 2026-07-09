--TEST--
gettext MO catalog lookup and plural selection
--EXTENSIONS--
gettext
--FILE--
<?php
function write_mo($path, $entries) {
    ksort($entries, SORT_STRING);
    $count = count($entries);
    $originalTable = 28;
    $translationTable = $originalTable + $count * 8;
    $stringOffset = $translationTable + $count * 8;
    $originalRows = '';
    $translationRows = '';
    $originalStrings = '';
    $translationStrings = '';

    foreach ($entries as $original => $translation) {
        $originalRows .= pack('VV', strlen($original), $stringOffset + strlen($originalStrings));
        $originalStrings .= $original . "\0";
    }

    $translationOffset = $stringOffset + strlen($originalStrings);
    foreach ($entries as $translation) {
        $translationRows .= pack('VV', strlen($translation), $translationOffset + strlen($translationStrings));
        $translationStrings .= $translation . "\0";
    }

    @mkdir(dirname($path), 0777, true);
    file_put_contents(
        $path,
        pack('V7', 0x950412de, 0, $count, $originalTable, $translationTable, 0, 0)
            . $originalRows
            . $translationRows
            . $originalStrings
            . $translationStrings
    );
}

$base = __DIR__ . '/gettext-mo-fixture';
write_mo($base . '/de_DE/LC_MESSAGES/messages.mo', [
    '' => "Content-Type: text/plain; charset=UTF-8\nPlural-Forms: nplurals=2; plural=(n != 1);\n",
    'Hello' => 'Hallo',
    "item\0items" => "Artikel\0Artikel plural",
]);
write_mo($base . '/de_DE/LC_CTYPE/messages.mo', [
    '' => "Content-Type: text/plain; charset=UTF-8\nPlural-Forms: nplurals=2; plural=(n != 1);\n",
    'Type' => 'Kategorie',
]);

putenv('LC_ALL=de_DE');
bindtextdomain('messages', $base);
textdomain('messages');

var_dump(gettext('Hello'));
var_dump(_('Hello'));
var_dump(dgettext('messages', 'Hello'));
var_dump(ngettext('item', 'items', 1));
var_dump(ngettext('item', 'items', 2));
var_dump(dngettext('messages', 'item', 'items', 2));
var_dump(dcgettext('messages', 'Type', LC_CTYPE));
var_dump(gettext('Missing'));
?>
--EXPECT--
string(5) "Hallo"
string(5) "Hallo"
string(5) "Hallo"
string(7) "Artikel"
string(14) "Artikel plural"
string(14) "Artikel plural"
string(9) "Kategorie"
string(7) "Missing"
