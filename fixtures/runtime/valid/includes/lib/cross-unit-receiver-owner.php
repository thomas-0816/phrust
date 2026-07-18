<?php
class CrossUnitReceiverOwner {
	private $target;

	public function add_values( $values ) {
		if ( null === $this->target ) {
			$this->target = new CrossUnitReceiverTarget( $values );
		}
		$this->target->add_values( $values );
		return $this->target instanceof CrossUnitReceiverTarget;
	}
}
