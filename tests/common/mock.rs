//! Mocking Actix actors.

use actix::{
    dev::channel::{AddressSender, AddressReceiver, channel},
    prelude::*,
};

pub struct Mocker<A: Actor> {
    tx: AddressSender<A>,
    rx: AddressReceiver<A>,
}

impl<A: Actor> Mocker<A> {
    /// Construct a new mocker.
    pub fn new() -> Self {
        let (tx, rx) = channel(0);

        Mocker { tx, rx }
    }

    /// Get address of a mock actor.
    pub fn addr(&self) -> Addr<A> {
        Addr::new(self.tx.clone())
    }
}
