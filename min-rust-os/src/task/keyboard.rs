use crate::{print, println};
use conquer_once::spin::OnceCell;
use core::{
    pin::Pin,
    task::{Context, Poll},
};
use crossbeam_queue::ArrayQueue;
use futures_util::stream::{Stream, StreamExt};
use futures_util::task::AtomicWaker;
use pc_keyboard::{layouts, DecodedKey, HandleControl, Keyboard, ScancodeSet1};

// Since the ArrayQueue::new performs a heap allocation, which is not possible
// at compile time (yet), we can't initialize the static variable directly.
// Instead, we use the OnceCell type of the conquer_once crate, which makes
// it possible to perform safe one-time initialization of static values.
static SCANCODE_QUEUE: OnceCell<ArrayQueue<u8>> = OnceCell::uninit();

static WAKER: AtomicWaker = AtomicWaker::new();

// Called by the keyboard interrupt handler
// Must not block or allocate
// Since this function should not be callable from our main.rs,
// use the pub(crate) visibility to make it only available to lib.rs
pub(crate) fn add_scancode(scancode: u8) {
    // use the OnceCell::try_get to get a reference to the initialized queue.
    // If the queue is not initialized yet, we ignore the keyboard scancode and
    // print a warning. It's important that we don't try to initialize the queue
    // in this function because it will be called by the interrupt handler,
    // which should not perform heap allocations.
    if let Ok(queue) = SCANCODE_QUEUE.try_get() {
        if let Err(_) = queue.push(scancode) {
            println!("WARNING: scancode queue fill; dropping keyboard input");
        } else {
            // add a call to WAKER.wake() if the push to the scancode queue succeeds.
            WAKER.wake();
        }
    } else {
        println!("WARNING: scancode queue uninitialised");
    }
}

// to initialise the `SCANCODE_QUEUE` and read the scancodes from the queue in an
// async way, use the following type
pub struct ScancodeStream {
    // the purpose of this field is to prevent constructiuons odf the struct from
    // outside of the module. This makes the new function the only way to construct
    // the type
    _private: (),
}

impl ScancodeStream {
    pub fn new() -> Self {
        SCANCODE_QUEUE
            .try_init_once(|| ArrayQueue::new(100))
            .expect("ScancodeStream::new should only be called once");
        ScancodeStream { _private: () }
    }
}

impl Stream for ScancodeStream {
    type Item = u8;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<u8>> {
        // e first use the OnceCell::try_get method to get a reference to the
        // initialized scancode queue. This should never fail since we initialize
        // the queue in the new function, so we can safely use the expect method
        // to panic if it's not initialized.
        let queue = SCANCODE_QUEUE.try_get().expect("not initialised");

        // Fast path. If it succeeds we return the scancode wrapped in Poll::Ready(Some(…)).
        // This way, we can avoid the performance overhead of registering a waker
        // when the queue is not empty.
        if let Ok(scancode) = queue.pop() {
            return Poll::Ready(Some(scancode));
        }

        WAKER.register(&cx.waker());
        // use the ArrayQueue::pop method to try to get the next element from the queue.
        // after registering the Waker contained in the passed Context through the
        // AtomicWaker::register function, we try popping from the queue a second time.
        // If it now succeeds, we return Poll::Ready.
        // If it fails, it means that the queue is empty. In that case, we return Poll::Pending.
        match queue.pop() {
            Ok(scancode) => {
                WAKER.take();
                Poll::Ready(Some(scancode))
            }
            Err(crossbeam_queue::PopError) => Poll::Pending,
        }
    }
}

pub async fn print_keypresses() {
    let mut scancodes = ScancodeStream::new();
    let mut keyboard = Keyboard::new(layouts::Us104Key, ScancodeSet1, HandleControl::Ignore);

    // instead of reading the scancode from an I/O port, we take it from the ScancodeStream
    // Use while let to loop until the stream returns None to signal its end.
    // Since our poll_next method never returns None, this is effectively an endless loop,
    // so the print_keypresses task never finishes. This also means that the CPU will be forever
    // occupied.
    while let Some(scancode) = scancodes.next().await {
        if let Ok(Some(key_event)) = keyboard.add_byte(scancode) {
            if let Some(key) = keyboard.process_keyevent(key_event) {
                match key {
                    DecodedKey::Unicode(character) => print!("{}", character),
                    DecodedKey::RawKey(key) => print!("{:?}", key),
                }
            }
        }
    }
}
