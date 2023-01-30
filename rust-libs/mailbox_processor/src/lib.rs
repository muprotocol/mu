use thiserror::Error;
use tokio::sync::{mpsc, oneshot};

pub mod callback;
pub mod plain;

#[derive(Debug, PartialEq, Eq, Error)]
pub enum Error {
    #[error("Mailbox is stopped")]
    MailboxStopped,
}

pub type Result<T> = std::result::Result<T, Error>;

// This function is used to ignore errors when sending something
// from inside the mailbox. The receiving end may have panicked,
// but we don't want to fail the entire mailbox if they do.
fn ignore_error<E>(_r: std::result::Result<(), E>) {}

/// Used to return data from a message.
///
/// `ReplyChannel`s are constructed by [`MailboxProcessor::post_and_reply`].
/// `ReplyChannel`s mut be embedded in the message itself.
/// The step function can take the `ReplyChannel` out of the message and use the
/// [`ReplyChannel::reply`] function to send back a reply.
///
/// The reply channel is also used to detect when the
/// message has been received and processed by the mailbox, so it's best to provide
/// the reply late in the message's processing logic. The reply channel should always
/// be used; otherwise, `post_and_reply` will report a `MailboxStopped` error when it's
/// dropped. See the documentation for [`MailboxProcessor`] for a code sample featuring
/// `ReplyChannel`s.
pub struct ReplyChannel<T> {
    sender: oneshot::Sender<T>,
}

impl<T> ReplyChannel<T> {
    pub fn reply(self, val: T) {
        ignore_error(self.sender.send(val));
    }
}

impl<T> std::fmt::Debug for ReplyChannel<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "<ReplyChannel of {}>", std::any::type_name::<T>())
    }
}

/// Can be used to raise notifications out of a mailbox. Notifications
/// aren't guaranteed to arrive, and the mailbox has no way to know
/// when (of if) they were received.
///
/// Use of this type is completely optional, but it implements the
/// best practices for raising notifications from mailboxes and its
/// use in this scenario is highly recommended.
#[derive(Clone)]
pub struct NotificationChannel<T> {
    sender: mpsc::UnboundedSender<T>,
}

impl<T> NotificationChannel<T> {
    pub fn new() -> (Self, mpsc::UnboundedReceiver<T>) {
        let (tx, rx) = mpsc::unbounded_channel();
        (Self { sender: tx }, rx)
    }

    pub fn send(&self, notification: T) {
        // Notifications aren't guaranteed to arrive, and we don't need to handle
        // closed receivers.
        let _ = self.sender.send(notification);
    }
}

/// Can be used to make a request to another task/thread and wait for
/// a reply.
///
/// This is, in essence, very similar to a mailbox; but it is meant for
/// scenarios where the request-handling end is not a mailbox to begin
/// with, such as when a child task needs to ask the task that spawned
/// it for some data. Mailboxes work the other way around, with the
/// mailbox being spawned by some parent task and receiving messages
/// from it.
pub struct RequestReplyChannel<Request, Reply> {
    sender: mpsc::UnboundedSender<(Request, ReplyChannel<Reply>)>,
}

impl<Request, Reply> Clone for RequestReplyChannel<Request, Reply> {
    fn clone(&self) -> Self {
        Self {
            sender: self.sender.clone(),
        }
    }
}

#[derive(Debug, Error)]
pub enum RequestReplyError {
    #[error("The receiver was dropped before the request could be sent")]
    ReceiverDropper,

    #[error("The reply channel was dropped while waiting for a reply")]
    ReplyChannelDropped,
}

impl<Request, Reply> RequestReplyChannel<Request, Reply> {
    pub fn new() -> (
        Self,
        mpsc::UnboundedReceiver<(Request, ReplyChannel<Reply>)>,
    ) {
        let (tx, rx) = mpsc::unbounded_channel();
        (Self { sender: tx }, rx)
    }

    pub async fn request(&self, request: Request) -> std::result::Result<Reply, RequestReplyError> {
        let (tx, rx) = oneshot::channel();
        let reply_channel = ReplyChannel { sender: tx };

        self.sender
            .send((request, reply_channel))
            .map_err(|_| RequestReplyError::ReceiverDropper)?;

        rx.await.map_err(|_| RequestReplyError::ReplyChannelDropped)
    }
}
