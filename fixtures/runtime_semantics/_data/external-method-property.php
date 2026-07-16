<?php

class ExternalMethodProperty
{
    public string $last_query = 'select 1';
    public bool $suppress_errors = false;

    public function get(): string
    {
        $this->nested('bad');
        global $EZSQL_ERROR;
        return $EZSQL_ERROR[0]['query'];
    }

    private function nested(string $message): void
    {
        global $EZSQL_ERROR;
        $EZSQL_ERROR[] = array(
            'query' => $this->last_query,
            'error_str' => $message,
        );
        if ($this->suppress_errors) {
            return;
        }
    }
}
