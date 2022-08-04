use std::future::Future;

use tokio::sync::{mpsc, oneshot};

use crate::*;

/// A MessageReceiver lets mailbox body functions
/// receive messages from the mailbox.
pub struct MessageReceiver<T: Send + 'static> {
    receiver: mpsc::Receiver<T>,
}

impl<T: Send + 'static> MessageReceiver<T> {
    /// Receive the next message from the mailbox.
    pub async fn receive(&mut self) -> Option<T> {
        self.receiver.recv().await
    }
}

/// Mailbox processor based on a plain, most likely looping function.
/// This type of mailbox lets you monitor other sources (aside from
/// messages sent to the mailbox), and react to those as well.
/// To receive messages, [`PlainMailboxProcessor::start`] creates a
/// [`MessageReceiver`] which you must use to receive messages from
/// the mailbox regularly. Not receiving messages for a long time
/// may fill the internal mspc channel's buffer.
///
/// ```
/// use tokio_mailbox_processor::*;
/// use tokio_mailbox_processor::plain::*;
///
/// enum Message {
///     Set(i32, ReplyChannel<()>),
///     Get(ReplyChannel<i32>),
/// }
///
/// async fn body(mut recv: MessageReceiver<Message>) {
///     let mut state = 0;
///     loop {
///         let msg = recv.receive().await;
///         match msg {
///             None => break, // All mailbox clones were dropped
///             Some(Message::Set(i, r)) => {
///                 state = i;
///                 r.reply(());
///             },
///             Some(Message::Get(rpl)) => {
///                 rpl.reply(state);
///             }
///         }
///     }
/// }
///
/// tokio_test::block_on(async {
///     let mailbox = PlainMailboxProcessor::start(body, 100);
///     mailbox.post_and_reply(|r| Message::Set(10, r)).await.unwrap();
///     assert_eq!(
///         mailbox.post_and_reply(|r| Message::Get(r)).await.unwrap(),
///         10
///     );
/// });
/// ```
pub struct PlainMailboxProcessor<T: Send + 'static> {
    sender: mpsc::Sender<T>,
}

impl<T: Send + 'static> Clone for PlainMailboxProcessor<T> {
    fn clone(&self) -> Self {
        Self {
            sender: self.sender.clone(),
        }
    }
}

impl<T: Send + 'static> PlainMailboxProcessor<T> {
    /// Starts a new `PlainMailboxProcessor`.
    ///
    /// # Arguments
    /// * `body` - The body function, which will be invoked by the mailbox and should keep running
    ///     for as long as the mailbox should be responsive.
    /// * `buffer_size` - The maximum number of messages which will be buffered while waiting for execution.
    ///     Excessive messages will block the sending task until there is room to buffer them.
    pub fn start<Body, Fut>(body: Body, buffer_size: usize) -> PlainMailboxProcessor<T>
    where
        Fut: Future<Output = ()> + Send + 'static,
        Body: FnOnce(MessageReceiver<T>) -> Fut + Send + Sync + 'static,
    {
        let (tx, rx) = mpsc::channel(buffer_size);

        tokio::spawn(async move {
            body(MessageReceiver { receiver: rx }).await;
        });

        Self { sender: tx }
    }

    /// Posts a message to the mailbox without waiting for the response. Note than
    /// the mailbox may be stopped and the message may never be processed at all.
    /// Also note that, unlike [`CallbackMailboxProcessor`], a `PlainMailboxProcessor`
    /// has no `post` function. To work around this, you must use [`post_and_reply`]
    /// with a `ReplyChannel<()>` and reply with `()` when all work on the message is
    /// done.
    pub fn post_and_forget(&self, msg: T) {
        let sender = self.sender.to_owned();
        tokio::spawn(async move {
            // We're 'forgetting' the result, so there's nowhere to report an error
            ignore_error(sender.send(msg).await);
        });
    }

    /// Posts a message to the mailbox and waits for a reply on the `ReplyChannel`.
    /// This function then returns the value sent back in the `ReplyChannel`. This is
    /// the correct way to get data out of the mailbox; `Arc`s and synchronization
    /// primitives are generally not needed when using a mailbox.
    pub async fn post_and_reply<TReply>(
        &self,
        make_msg: impl FnOnce(ReplyChannel<TReply>) -> T,
    ) -> Result<TReply> {
        let (tx, rx) = oneshot::channel::<TReply>();
        let msg = make_msg(ReplyChannel { sender: tx });

        self.sender
            .send(msg)
            .await
            .map_err(|_| Error::MailboxStopped)?;

        // A Stop call may have already been posted to the mailbox,
        // so we may fail to read a reply back
        rx.await.map_err(|_| Error::MailboxStopped)
    }
}

#[cfg(test)]
mod tests {
    use std::{
        sync::{Arc, Mutex},
        time::Duration,
    };

    use crate::plain::*;

    type MailboxState = Arc<Mutex<i32>>;

    enum Message {
        Increment(i32, ReplyChannel<()>),
        Decrement(i32, ReplyChannel<()>),
        Get(ReplyChannel<i32>),
        Stop(ReplyChannel<()>),
        DelayAndFail,
    }

    async fn body(mut recv: MessageReceiver<Message>, state: MailboxState) {
        loop {
            let msg = recv.receive().await;
            match msg {
                None => return,

                Some(Message::Increment(i, r)) => {
                    let mut m = state.lock().unwrap();
                    *m += i;
                    r.reply(());
                }
                Some(Message::Decrement(i, r)) => {
                    let mut m = state.lock().unwrap();
                    *m -= i;
                    r.reply(());
                }
                Some(Message::Get(rep)) => {
                    let m = state.lock().unwrap();
                    rep.reply(*m);
                }
                Some(Message::Stop(r)) => {
                    r.reply(());
                    break;
                }
                Some(Message::DelayAndFail) => {
                    tokio::time::sleep(Duration::from_secs(10)).await;
                    panic!("OH MY GOD WE'RE DEAD");
                }
            }
        }
    }

    fn make_mb() -> (PlainMailboxProcessor<Message>, MailboxState) {
        let state = Arc::new(Mutex::new(0));
        let state_clone = state.clone();
        let mb = PlainMailboxProcessor::start(|r| body(r, state_clone), 100);
        (mb, state)
    }

    #[tokio::test]
    async fn messages_arrive_in_mailbox() -> Result<()> {
        let (mb, state) = make_mb();

        mb.post_and_reply(|r| Message::Increment(1, r)).await?;

        assert_eq!(*state.lock().unwrap(), 1);

        Ok(())
    }

    #[tokio::test]
    async fn can_get_replies() -> Result<()> {
        let (mb, _) = make_mb();

        mb.post_and_reply(|r| Message::Increment(5, r)).await?;
        let current = mb.post_and_reply(|tx| Message::Get(tx)).await?;

        assert_eq!(current, 5);

        Ok(())
    }

    #[tokio::test]
    async fn messages_arrive_in_order() -> Result<()> {
        let (mb, _) = make_mb();

        mb.post_and_reply(|r| Message::Increment(5, r)).await?;
        assert_eq!(mb.post_and_reply(|tx| Message::Get(tx)).await?, 5);

        mb.post_and_reply(|r| Message::Decrement(15, r)).await?;
        assert_eq!(mb.post_and_reply(|tx| Message::Get(tx)).await?, -10);

        mb.post_and_reply(|r| Message::Increment(20, r)).await?;
        assert_eq!(mb.post_and_reply(|tx| Message::Get(tx)).await?, 10);

        Ok(())
    }

    #[tokio::test]
    async fn post_and_forget_does_not_wait_for_a_result() -> Result<()> {
        let (mb, state) = make_mb();

        mb.post_and_forget(Message::DelayAndFail);

        // The state should still be zero, and the mutex shouldn't be poisoned
        assert_eq!(*state.lock().unwrap(), 0);

        Ok(())
    }

    #[tokio::test]
    async fn can_stop_mailbox_by_returning_from_body_function() -> Result<()> {
        let (mb, _) = make_mb();

        mb.post_and_reply(|r| Message::Increment(5, r)).await?;
        assert_eq!(mb.post_and_reply(|tx| Message::Get(tx)).await?, 5);

        mb.post_and_reply(|r| Message::Stop(r)).await?;

        assert_eq!(
            mb.post_and_reply(|r| Message::Increment(10, r)).await,
            Err(Error::MailboxStopped)
        );

        Ok(())
    }
}
