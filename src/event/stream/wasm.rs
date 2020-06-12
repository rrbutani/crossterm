use std::{
    collections::VecDeque,
    pin::Pin,
    task::{Context, Poll, Waker},
    sync::{Arc, RwLock as StdRwLock},
};

use futures::Stream;
// use parking_log::{
//     lock_api::{RawRwLock as RawRwLockTrait, RwLock as ParkingLogRwLock},
//     RawRwLock,
// };
use wasm_bindgen::prelude::Closure;
use xterm_js_sys::{
    xterm::{Disposable, ResizeEventData, Str, Terminal},
    ext::disposable::DisposableWrapper,
};

use crate::Result;

use super::super::{
    filter::EventFilter, Event, InternalEvent,
    sys::unix::parse::parse_event, // TODO: spin out parse instead...
};

// // Since we're single threaded/cooperatively scheduled we shouldn't actually
// // need a RwLock here (because we don't hold the lock across suspend points),
// // but w/e. This lock is supposed to work on wasm.
// static WAKER: RwLock<RawRwLock, Option<Waker>> = RwLock::const_new(
//     <RawRwLock as RawRwLockTrait>::INIT,
//     None,
// );

// Actually let's not put this in a global variable (since we _could_ have
// multiple terminals running at a time).
//
// Downside is that we have no way to detect when we're re-registering an
// EventStream on a Terminal.

// impl Drop for EventStream {
//    fn drop(&mut self) {
//        let _ = WAKER.write().unwrap().take();
//    }
// }

// type RwLock<T> = ParkingLogRwLock<RawRwLock, T>;
type RwLock<T> = StdRwLock<T>;

#[derive(Debug)]
pub struct EventStream<'t> {
    // On `Drop` this will automatically get unregistered.
    data_event_listener_handle: DisposableWrapper<Disposable>,
    resize_event_listener_handle: DisposableWrapper<Disposable>,
    // We store these here because these callbacks need to be somewhere in
    // memory while we're still listening for events and since we stop listening
    // by dropping this type, things work out nicely.
    data_event_closure: Closure<dyn FnMut(Str)>,
    resize_event_closure: Closure<dyn FnMut(ResizeEventData)>,

    waker: Arc<RwLock<Option<Waker>>>,
    events: Arc<RwLock<VecDeque<Result<InternalEvent>>>>,

    terminal: &'t Terminal,
}


// Note: another way to do this would be to just hold a reference to the
// Terminal and to register the callbacks (with the waker built-in) in
// `poll_next`.
//
// This would let us get rid of the `Arc<RwLock<Option<Waker>>>` thing and
// it'd make us hold onto the `Terminal` to make sure that it doesn't go out
// of scope before the `EventStream` does (we do this anyways but we don't
// actually do anything with the terminal).

impl<'t> EventStream<'t> {
    // Warning: This does not check if the terminal already has an event stream
    // registered to it. If it does, registering a new EventStream to the
    // terminal will break the existing EventStream.
    fn new(term: &'t Terminal) -> Self {
        let waker = Arc::new(RwLock::new(None));
        let events = Arc::new(RwLock::new(VecDeque::with_capacity(64)));

        let (data_event_closure, data_event_listener_handle) = {
            let (waker, events) = (waker.clone(), events.clone());

            let clos: Box<dyn FnMut(_)> = Box::new(move |data: Str| {
                let events = events.write().unwrap();
                let buffer = Vec::with_capacity(10);
                let bytes = data.as_bytes();

                for (idx, byte) in bytes.iter().enumerate() {
                    let more = idx + 1 < bytes.len();
                    match parse_event(&buffer, more) {
                        Ok(Some(ev)) => {
                            events.push_back(Ok(ev));
                            buffer.clear();
                        }
                        Ok(None) => {
                            // Add some more bytes and try again..
                        }
                        Err(err) => {
                            // Store the error and clear the buffer.
                            events.push_back(Err(err));
                            buffer.clear();
                        }
                    }
                }

                waker.read().unwrap().as_ref().map(|w| w.wake_by_ref())
            });
            let clos = Closure::wrap(clos);

            (clos, term.on_data(&clos).into())
        };

        let (resize_event_closure, resize_event_listener_handle) = {
            let (waker, events) = (waker.clone(), events.clone());
            let clos: Box<dyn FnMut(_)> = Box::new(move |ev: ResizeEventData| {
                let events = events.write().unwrap();
                events.push_back(Ok(Event::Resize(ev.cols(), ev.rows())));

                waker.read().unwrap().as_ref().map(|w| w.wake_by_ref())
            });
            let clos = Closure::wrap(clos);

            (clos, term.on_resize(&clos).into())
        };

        // let data_waker = waker.clone();
        // let data_event_closure: Box<dyn FnMut(Str)> = Box::new(move |data: Str| {

        //     data_waker.read().unwrap().as_ref().map(|w| w.wake_by_ref());
        // });
        // let data_event_closure = Closure::wrap(data_event_closure);
        // let data_event_listener_handle = term.on_data(&data_event_closure);

        // let resize_waker = waker.clone();
        // let resize_event_closure: Box<dyn FnMut(ResizeEventData)> = Box::new(move |ev: ResizeEventData| {

        //     resize_waker.read().unwrap().as_ref().map(|w| w.wake_by_ref());
        // });
        // let resize_event_closure = Closure::wrap(resize_event_closure);
        // let resize_event_listener_handle = term.on_resize(&resize_event_closure);

        Self {
            data_event_listener_handle,
            resize_event_listener_handle,

            data_event_closure,
            resize_event_closure,

            waker,
            events,

            terminal: term,
        }
    }
}

impl<'t> Stream for EventStream<'t> {
    type Item = Result<Event>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if self.waker.read().unwrap().is_none() {
            *self.waker.write().unwrap() = Some(cx.waker().clone())
        }

        let events = self.waker.write().unwrap();

        // Filter out CursorPosition events here:
        loop {
            match events.pop_front() {
                Some(Err(err)) => break Poll::Ready(Some(Err(err))),
                Some(Ok(InternalEvent::Event(ev))) => break Poll::Ready(Some(Ok(ev))),
                Some(Ok(InternalEvent::CursorPosition(_, _))) => continue,
                // Some(Ok(ev)) => {
                //     if EventFilter.eval(&ev) {
                //         break Poll::Ready(Some(Ok(ev)))
                //     } else {
                //         continue
                //     }
                // }
                None => break Poll::Pending,
            }
        }
    }
}
