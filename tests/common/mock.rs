//! Mocking Actix actors.

use actix::{
    dev::channel::{AddressSender, AddressReceiver, channel},
    prelude::*,
};

pub struct Mocker<A: Actor> {
    tx: AddressSender<A>,
    _rx: AddressReceiver<A>,
}

impl<A: Actor> Mocker<A> {
    /// Construct a new mocker.
    pub fn new() -> Self {
        let (tx, _rx) = channel(0);

        Mocker { tx, _rx }
    }

    /// Get address of a mock actor.
    pub fn addr(&self) -> Addr<A> {
        Addr::new(self.tx.clone())
    }
}
