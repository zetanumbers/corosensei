// FIXME: add safety comments
// FIXME: handle unwinding
use core::{
    future::{Future, IntoFuture},
    pin::{pin, Pin},
    ptr, task,
};

use super::Fiber;

pub struct FiberTaskContext<T> {
    cx: *mut task::Context<'static>,
    parent: Fiber<FiberPoll<T>>,
}

impl<T> FiberTaskContext<T> {
    // TODO: consider this approach to allow non-static arguments in fibers
    pub fn cx<'a>(&'a mut self) -> &'a mut task::Context<'a> {
        unsafe { &mut *self.cx.cast::<task::Context<'a>>() }
    }
}

impl<T: 'static> FiberTaskContext<T> {
    pub fn yield_pending(self) -> Self {
        self.parent.switch(FiberPoll::Pending)
    }

    pub fn yield_custom(self, poll: task::Poll<T>) -> Self {
        self.parent.switch(|fib| match poll {
            task::Poll::Ready(r) => FiberPoll::Ready(r),
            task::Poll::Pending => FiberPoll::Pending(fib),
        })
    }
}

pub trait FiberAwait {
    type Output;
    fn await_(
        self,
        cx: FiberTaskContext<Self::Output>,
    ) -> (Self::Output, FiberTaskContext<Self::Output>);
    // TODO: await_ with `&mut cx`?
}

impl<Fut: IntoFuture> FiberAwait for Fut
where
    Fut::Output: 'static,
{
    type Output = Fut::Output;

    fn await_(
        self,
        mut cx: FiberTaskContext<Self::Output>,
    ) -> (Self::Output, FiberTaskContext<Self::Output>) {
        let mut fut = pin!(self.into_future());
        loop {
            match fut.as_mut().poll(cx.cx()) {
                task::Poll::Ready(out) => return (out, cx),
                task::Poll::Pending => cx = cx.yield_pending(),
            }
        }
    }
}

pub enum FiberPoll<T> {
    Ready(T),
    Pending(Fiber<FiberTaskContext<T>>),
}

impl<T: 'static> IntoFuture for Fiber<FiberTaskContext<T>> {
    type Output = T;

    type IntoFuture = FiberFuture<T>;

    fn into_future(self) -> Self::IntoFuture {
        todo!()
    }
}

pub struct FiberFuture<T> {
    inner: Option<Fiber<FiberTaskContext<T>>>,
}

impl<T: 'static> Future for FiberFuture<T> {
    type Output = T;

    fn poll(mut self: Pin<&mut Self>, cx: &mut task::Context<'_>) -> task::Poll<Self::Output> {
        let cx = ptr::addr_of_mut!(*cx).cast::<task::Context<'static>>();
        let poll = self
            .inner
            .take()
            .expect("FiberFuture have already completed or unwinded")
            // SAFETY: TODO
            .switch(move |parent| FiberTaskContext { cx, parent });
        match poll {
            FiberPoll::Ready(out) => task::Poll::Ready(out),
            FiberPoll::Pending(fiber) => {
                self.inner = Some(fiber);
                task::Poll::Pending
            }
        }
    }
}
