<?php

class ExternalCompactProcessor
{
    public function insert($table, $data, $format = null)
    {
        return $this->insertHelper($table, $data, $format);
    }

    protected function insertHelper($table, $data, $format = null)
    {
        return $this->process($data, $format);
    }

    protected function process($data, $format)
    {
        foreach ($data as $field => $value) {
            $data[$field] = array(
                'value' => $value,
                'format' => '%s',
            );
        }
        return $data;
    }
}

function external_compact_by_value($value, $kind = 'comment')
{
    return $kind . ':' . $value;
}
