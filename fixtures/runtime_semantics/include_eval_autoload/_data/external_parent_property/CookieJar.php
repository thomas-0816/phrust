<?php
class CookieJar
{
    public function optionalReference(&$found = null): bool
    {
        $found = true;
        return $found;
    }

    public function beforeRequest($url, &$headers, &$data, &$type, &$options): void
    {
        $headers['cookie'] = 'fixture=1';
        $options['seen'] = $type . ':' . $url;
    }
}
