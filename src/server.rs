// MIT/Apache2 License

use crate::{directive::Directive, event::Event};
use breadthread::{BreadThread, Completer, Controller, PinnedThreadHandle, ThreadHandle};
use orphan_crippler::Receiver;
use std::{
    cell::RefCell,
    collections::{HashMap, VecDeque},
    iter,
    mem::{self, MaybeUninit},
    rc::Rc,
};
use winapi::{
    shared::minwindef::{DWORD, LPARAM, UINT},
    um::{
        processthreadsapi::GetCurrentThreadId,
        winuser::{GetMessage, PostThreadMessageA, TranslateMessage, MSG, WM_APP},
    },
};

/// The GUI thread for `Yaww`. This object is what you use in order to do GUI operations.
#[repr(transparent)]
pub struct GuiThread<'evh> {
    inner: BreadThread<'evh, YawwController<'evh>>,
}

impl<'evh> GuiThread<'evh> {
    /// Create a new `GuiThread` on this thread.
    #[inline]
    pub fn new() -> Self {
        let mut bt = BreadThread::new(YawwControler {
            handle: None,
            window_count: Cell::new(-1),
            subclasses: RefCell::new(HashMap::new()),
        });
        let th = bt.handle().pin();
        bt.with_mut(|ctrl| {
            ctrl.handle = Some(PinnedGuiThreadHandle { inner: th });
        });
        bt
    }

    /// Get a handle to this `GuiThread` that can be sent between threads.
    #[inline]
    pub fn handle(&self) -> GuiThreadHandle<'evh> {
        GuiThreadHandle {
            inner: self.inner.handle(),
        }
    }

    #[inline]
    pub(crate) fn send_directive<T: Any + Send>(
        &self,
        directive: Directive,
    ) -> crate::Result<Receiver<T>> {
        let r = self.inner.send_directive(directive)?;
        Ok(r)
    }

    /// Enter the main loop.
    #[inline]
    pub fn main_loop(self) -> crate::Result {
        self.inner.main_loop()?;
        Ok(())
    }
}

/// Handle to the GUI thread.
#[derive(Clone)]
#[repr(transparent)]
pub struct GuiThreadHandle<'evh> {
    inner: ThreadHandle<'evh, YawwController<'evh>>,
}

impl<'evh> GuiThreadHandle<'evh> {
    #[inline]
    pub(crate) fn send_directive<T: Any + Send>(
        &self,
        directive: Directive,
    ) -> crate::Result<Receiver<T>> {
        let r = self.inner.send_directive(directive)?;
        Ok(r)
    }

    #[inline]
    pub fn pin(self) -> PinnedGuiThreadHandle<'evh> {
        PinnedGuiThreadHandle {
            inner: self.inner.pin(),
        }
    }
}

/// Pinned handle to the GUI thread.
#[derive(Clone)]
#[repr(transparent)]
pub struct PinnedGuiThreadHandle<'evh> {
    inner: PinnedThreadHandle<'evh, YawwController<'evh>>,
}

impl<'evh> PinnedGuiThreadHandle<'evh> {
    #[inline]
    pub(crate) fn send_directive<T: Any + Send>(
        &self,
        directive: Directive,
    ) -> crate::Result<Receiver<T>> {
        let r = self.inner.send_directive(directive)?;
        r
    }

    #[inline]
    pub(crate) fn set_event_handler<F: FnMut(&YawwController<'evh>, Event) + Send>(&self, f: F) {
        self.inner.set_event_handler(f);
    }

    #[inline]
    pub fn into_inner(self) -> GuiThreadHandle<'evh> {
        GuiThreadHandle {
            inner: self.inner.into_inner(),
        }
    }

    /// Convert into a pointer.
    #[inline]
    pub(crate) fn into_raw(self) -> *const () {
        self.inner.into_raw()
    }

    /// Retrieve from a pointer, UB if not from into_raw()
    #[inline]
    pub(crate) unsafe fn from_raw(ptr: *const ()) -> Self {
        Self {
            inner: PinnedThreadHandle::from_raw(ptr),
        }
    }
}

/// Trait for telling if an object can send a directive.
pub trait SendsDirective {
    /// Sends a directive.
    #[doc(hidden)]
    fn send<T: Any + Send>(&self, directive: Directive) -> crate::Result<Receiver<T>>;
}

impl<'evh> SendsDirective for GuiThread<'evh> {
    #[inline]
    fn send<T: Any + Send>(&self, directive: Directive) -> crate::Result<Receiver<T>> {
        self.send_directive(directive)
    }
}

impl<'evh> SendsDirective for GuiThreadHandle<'evh> {
    #[inline]
    fn send<T: Any + Send>(&self, directive: Directive) -> crate::Result<Receiver<T>> {
        self.send_directive(directive)
    }
}

impl<'evh> SendsDirective for PinnedGuiThreadHandle<'evh> {
    #[inline]
    fn send<T: Any + Send>(&self, directive: Directive) -> crate::Result<Receiver<T>> {
        self.send_directive(directive)
    }
}

const WM_DIRECTIVE: UINT = WM_APP + 0x1337;

/// The inner controller type. Should not be exposed to the public.
pub(crate) struct YawwControler<'evh> {
    /// An inner handle to the bread thread to send directives with.    
    handle: Option<PinnedGuiThreadHandle<'evh>>,
    /// The current number of windows. -1 represents windows not being uninit.
    window_count: Cell<isize>,
    /// Hash map of window IDs to window subclasses.
    subclasses: RefCell<
        HashMap<NonZeroUsize, unsafe extern "system" fn(HWND, UINT, WPARAM, LPARAM) -> LRESULT>,
    >,
}

impl<'evh> YawwController<'evh> {
    #[inline]
    pub(crate) fn handle(&self) -> &PinnedGuiThreadHandle<'evh> {
        self.handle
            .as_ref()
            .expect("handle should never be None during normal operation")
    }

    #[inline]
    pub(crate) fn increment_window_count(&self) -> isize {
        let window_count = match self.window_count.get() {
            -1 => 1,
            count => count + 1,
        };
        self.window_count.set(window_count);
        window_count
    }

    #[inline]
    pub(crate) fn decrement_window_count(&self) -> isize {
        let window_count = self.window_count.get() - 1;
        self.window_count.set(window_count);
        window_count
    }

    #[inline]
    pub(crate) fn insert_subclass(
        &self,
        id: NonZeroUsize,
        subclass: unsafe extern "system" fn(HWND, UINT, WPARAM, LPARAM) -> LRESULT,
    ) {
        self.subclasses.borrow_mut().insert(id, subclass);
    }

    #[inline]
    pub(crate) fn subclass(
        &self,
        id: NonZeroUsize,
    ) -> Option<unsafe extern "system" fn(HWND, UINT, WPARAM, LPARAM) -> LRESULT> {
        self.subclasses.borrow().get(&id).copied()
    }
}

impl<'evh> Controller for YawwController<'evh> {
    type Directive = Directive;
    type DirectiveAdaptor = YawwDirectiveAdaptor;
    type Error = crate::Error;
    type Event = Event;
    type Pointers = iter::Once<AddOrRemovePtr>;

    #[inline]
    fn directive_adaptor(&mut self) -> DirectiveAdaptor {
        // SAFETY: all this does is get the thread ID
        YawwDirectiveAdaptor {
            thread_id: unsafe { GetCurrentThreadId() },
        }
    }

    #[inline]
    fn loop_cycle(&mut self) -> Result<LoopCycle<Event, Directive>, crate::Error> {
        let mut msg = MaybeUninit::<MSG>::uninit();
        loop {
            // SAFETY: https://docs.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-getmessage
            match unsafe { GetMessage(msg.as_mut_ptr(), ptr::null_mut(), 0, 0) } {
                -1 => {
                    // get message had an error
                    return Err(crate::win32_error(Some("GetMessage")));
                }
                0 => {
                    // we got the quit message; return a break
                    return Ok(LoopCycle::Break);
                }
                _ => {
                    // SAFETY: if GetMessage returned non-0 and non-(-1), msg is init
                    let msg = MaybeUninit::assume_init(msg);

                    if msg == WM_DIRECTIVE {
                        // SAFETY: this is our user-defined event, where lparam is a Box<Sender<Directive>>
                        let sender: Box<Sender<Directive>> =
                            unsafe { Box::from_raw(sender.lparam as *mut _) };
                        return Ok(LoopCycle::Directive(*sender));
                    } else {
                        // SAFETY: it is well-defined to do these things with an msg pointer
                        unsafe {
                            TranslateMessage(&msg);
                            DispatchMessage(&msg);
                        }
                    }
                }
            }
        }
    }

    #[inline]
    fn process_directive<C: Completer>(
        &self,
        directive: Directive,
        completer: &mut C,
    ) -> Self::Pointers {
        iter::once(directive.process(self, completer))
    }
}

#[repr(transparent)]
struct YawwDirectiveAdaptor {
    thread_id: DWORD,
}

impl DirectiveAdaptor<Directive> for YawwDirectiveAdaptor {
    #[inline]
    fn send(&mut self, directive: Sender<Directive>) {
        // box up the directive
        let directive = Box::new(directive);
        // turn it into a pointer, then an isize, so it can be sent
        let directive = Box::into_raw(directive) as LPARAM;

        // send it!
        unsafe {
            // SAFETY: https://docs.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-postthreadmessagea
            PostThreadMessageA(self.thread_id, WM_DIRECTIVE, 0, directive)
        }
    }
}
