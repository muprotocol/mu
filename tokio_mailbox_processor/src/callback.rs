use std::future::Future;

use tokio::sync::{mpsc, oneshot};

use crate::*;

enum ControlMessage<T> {
    UserMessageAndAck(T, oneshot::Sender<()>),
    UserMessage(T),
    StopAndAck(oneshot::Sender<()>),
    Stop,
}

/// Mailbox processor based on a callback function.
///
/// A `CallbackMailboxProcessor` can be created by calling
/// [`CallbackMailboxProcessor::start`]. `CallbackMailboxProcessor`s can be cloned and
/// used from multiple threads. The `CallbackMailboxProcessor` will always process
/// messages in the order they arrive (subject to the semantics of
/// [`tokio::sync::mpsc`]).
///
/// To use a `CallbackMailboxProcessor`, one must first create a message type,
/// normally an enum representing the different messages the mailbox can
/// handle. The mailbox will then handle messages using a step function.
/// This function is called once per message.
///
/// A mailbox also has internal state, which is persisted between
/// consecutive messages. Each invocation of the step function returns the
/// updated state, in functional programming fashion. The initial state must
/// be provided to [`CallbackMailboxProcessor::start`], which is then fed into the
/// first invocation of the step function. Each consecutive invocation of the
/// step function then gets the updated state from the previous invocation.
///
/// ```
/// use tokio_mailbox_processor::*;
/// use tokio_mailbox_processor::callback::*;
///
/// enum Message {
///     Set(i32),
///     Get(ReplyChannel<i32>),
/// }
///
/// async fn step(msg: Message, state: i32) -> i32 {
///     match msg {
///         Message::Set(i) => i,
///         Message::Get(rpl) => {
///             rpl.reply(state);
///             state
///         }
///     }
/// }
///
/// tokio_test::block_on(async {
///     let mailbox = CallbackMailboxProcessor::start(step, 0, 100);
///     mailbox.post(Message::Set(10)).await.unwrap();
///     assert_eq!(
///         mailbox.post_and_reply(|r| Message::Get(r)).await.unwrap(),
///         10
///     );
/// });
/// ```
pub struct CallbackMailboxProcessor<T: Send + 'static> {
    sender: mpsc::Sender<ControlMessage<T>>,
}

impl<T: Send + 'static> Clone for CallbackMailboxProcessor<T> {
    fn clone(&self) -> Self {
        Self {
            sender: self.sender.clone(),
        }
    }
}

impl<T: Send + 'static> CallbackMailboxProcessor<T> {
    /// Starts a new `CallbackMailboxProcessor`.
    ///
    /// # Arguments
    /// * `step` - The step function, invoked once per message.
    /// * `init_state` - The initial state of the mailbox.
    /// * `buffer_size` - The maximum number of messages which will be buffered while waiting for execution.
    ///     Excessive messages will block the sending task until there is room to buffer them.
    pub fn start<State: Send + 'static, Step, Fut>(
        step: Step,
        init_state: State,
        buffer_size: usize,
    ) -> CallbackMailboxProcessor<T>
    where
        Fut: Future<Output = State> + Send + 'static,
        Step: Fn(T, State) -> Fut + Send + Sync + 'static,
    {
        let (tx, mut rx) = mpsc::channel(buffer_size);

        tokio::spawn(async move {
            let mut state = init_state;

            while let Some(msg) = rx.recv().await {
                match msg {
                    ControlMessage::StopAndAck(tx) => {
                        rx.close();
                        ignore_error(tx.send(()));
                        break;
                    }

                    ControlMessage::Stop => {
                        rx.close();
                        break;
                    }

                    ControlMessage::UserMessageAndAck(msg, tx) => {
                        state = step(msg, state).await;

                        match tx.send(()) {
                            // If the rx end panics while we process the message (which shouldn't happen),
                            // we don't want to stop the entire mailbox
                            _ => (),
                        }
                    }

                    ControlMessage::UserMessage(msg) => {
                        state = step(msg, state).await;
                    }
                }
            }
        });

        Self { sender: tx }
    }

    /// Stops (and consumes) the mailbox processor without waiting for it to be stopped.
    /// See [`MailboxProcessor::stop`].
    pub fn stop_and_forget(self) {
        tokio::spawn(async move {
            // We're 'forgetting' the result, so there's nowhere to report an error
            ignore_error(self.sender.send(ControlMessage::Stop).await);
        });
    }

    /// Stops (and consumes) the mailbox processor and waits for it to be stopped.
    /// Note that other copies of the mailbox processor may still be alive and will return
    /// [`Error::MailboxStopped`] when used to send messages.
    /// Both `stop` and `stop_and_forget` are idempotent, so stop can be called on each copy
    /// of the mailbox.
    /// The mailbox will also be automatically stopped when no more copies of it are alive.
    pub async fn stop(self) {
        // No need to handle errors on either call, since if the channel
        // is stopped, we're just trying to stop it again which is OK
        let (tx, rx) = oneshot::channel();
        ignore_error(self.sender.send(ControlMessage::StopAndAck(tx)).await);
        ignore_error(rx.await);
    }

    /// Posts a message to the mailbox without waiting for the response. Note than
    /// the mailbox may be stopped and the message may never be processed at all.
    pub fn post_and_forget(&self, msg: T) {
        let sender = self.sender.to_owned();
        tokio::spawn(async move {
            // We're 'forgetting' the result, so there's nowhere to report an error
            ignore_error(sender.send(ControlMessage::UserMessage(msg)).await);
        });
    }

    /// Posts a message to the mailbox and waits for it to be processed before
    /// returning.
    pub async fn post(&self, msg: T) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        self.sender
            .send(ControlMessage::UserMessageAndAck(msg, tx))
            .await
            .map_err(|_| Error::MailboxStopped)?;

        // A Stop call may have already been posted to the mailbox,
        // so we may fail to read a reply back
        rx.await.map_err(|_| Error::MailboxStopped)
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
            .send(ControlMessage::UserMessage(msg))
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

    use crate::callback::*;

    type MailboxState = Arc<Mutex<i32>>;

    enum Message {
        Increment(i32),
        Decrement(i32),
        Get(ReplyChannel<i32>),
        DelayAndFail,
    }

    async fn step(msg: Message, state: MailboxState) -> MailboxState {
        match msg {
            Message::Increment(i) => {
                let mut m = state.lock().unwrap();
                *m += i;
            }
            Message::Decrement(i) => {
                let mut m = state.lock().unwrap();
                *m -= i;
            }
            Message::Get(rep) => {
                let m = state.lock().unwrap();
                rep.reply(*m);
            }
            Message::DelayAndFail => {
                tokio::time::sleep(Duration::from_secs(10)).await;
                panic!("OH MY GOD WE'RE DEAD");
            }
        }

        state
    }

    fn make_mb() -> (CallbackMailboxProcessor<Message>, MailboxState) {
        let state = Arc::new(Mutex::new(0));
        let mb = CallbackMailboxProcessor::start(step, state.clone(), 100);
        (mb, state)
    }

    #[tokio::test]
    async fn messages_arrive_in_mailbox() -> Result<()> {
        let (mb, state) = make_mb();

        mb.post(Message::Increment(1)).await?;

        assert_eq!(*state.lock().unwrap(), 1);

        Ok(())
    }

    #[tokio::test]
    async fn can_get_replies() -> Result<()> {
        let (mb, _) = make_mb();

        mb.post(Message::Increment(5)).await?;
        let current = mb.post_and_reply(|tx| Message::Get(tx)).await?;

        assert_eq!(current, 5);

        Ok(())
    }

    #[tokio::test]
    async fn messages_arrive_in_order() -> Result<()> {
        let (mb, _) = make_mb();

        mb.post(Message::Increment(5)).await?;
        assert_eq!(mb.post_and_reply(|tx| Message::Get(tx)).await?, 5);

        mb.post(Message::Decrement(15)).await?;
        assert_eq!(mb.post_and_reply(|tx| Message::Get(tx)).await?, -10);

        mb.post(Message::Increment(20)).await?;
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
    async fn can_stop_mailbox() -> Result<()> {
        let (mb, _) = make_mb();

        mb.post(Message::Increment(5)).await?;
        assert_eq!(mb.post_and_reply(|tx| Message::Get(tx)).await?, 5);

        let mb2 = mb.clone();
        mb.stop().await;

        assert_eq!(
            mb2.post(Message::Increment(10)).await,
            Err(Error::MailboxStopped)
        );

        Ok(())
    }
    #[tokio::test]
    async fn can_stop_mailbox_multiple_times() -> Result<()> {
        let (mb, _) = make_mb();

        mb.post(Message::Increment(5)).await?;
        assert_eq!(mb.post_and_reply(|tx| Message::Get(tx)).await?, 5);

        let mb2 = mb.clone();
        let mb3 = mb.clone();
        let mb4 = mb.clone();
        let mb5 = mb.clone();

        mb.stop().await;

        assert_eq!(
            mb2.post(Message::Increment(10)).await,
            Err(Error::MailboxStopped)
        );
        mb2.stop().await;

        mb3.stop().await;
        mb4.stop().await;

        assert_eq!(
            mb5.post(Message::Increment(10)).await,
            Err(Error::MailboxStopped)
        );

        Ok(())
    }
}
