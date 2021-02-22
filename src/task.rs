// MIT/Apache2 License

use crate::directive::Directive;
use orphan_crippler::{two, Receiver, Sender};
use std::any::Any;

pub type Task<T> = Receiver<T>;
pub(crate) type ServerTask = Sender<Directive>;

#[inline]
pub(crate) fn create_task<T: Any + Send>(dir: Directive) -> (Task<T>, ServerTask) {
    let (send, recv) = two(dir);
    (recv, send)
}
