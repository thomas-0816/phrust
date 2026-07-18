<?php
class CrossUnitReceiverTarget {
	private $values = array();

	public function __construct( $values ) {
		$this->add_values( $values );
	}

	public function add_values( $values ) {
		$this->values = $values;
		return $this;
	}
}
