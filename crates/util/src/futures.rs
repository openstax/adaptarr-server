use futures::{Async, AsyncSink, Sink, StartSend, Poll};
use std::marker::PhantomData;

/// Create an instance of a [`Sink`] which will successfully consume all data.
pub fn void<I, E>() -> Void<I, E> {
    Void(PhantomData, PhantomData)
}

pub struct Void<I, E>(PhantomData<*const I>, PhantomData<*const E>);

impl<I, E> Sink for Void<I, E> {
    type SinkItem = I;
    type SinkError = E;

    fn start_send(&mut self, _: I) -> StartSend<I, E> {
        Ok(AsyncSink::Ready)
    }

    fn poll_complete(&mut self) -> Poll<(), E> {
        Ok(Async::Ready(()))
    }
}
