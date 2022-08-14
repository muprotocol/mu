// TODO: re-implement these over the new gossip code
#[cfg(test)]
mod test {
    use super::*;
    use futures::task::{Context, Poll};
    use futures::Sink;
    use pin_project::pin_project;
    use std::collections::VecDeque;
    use std::pin::Pin;

    #[pin_project]
    pub struct SinkStream<Si, St> {
        #[pin]
        sink: Si,

        #[pin]
        stream: St,
    }

    impl<Si, St> SinkStream<Si, St> {
        pub fn new(sink: Si, stream: St) -> SinkStream<Si, St> {
            SinkStream { sink, stream }
        }

        pub fn _stream(&self) -> &St {
            &self.stream
        }

        pub fn sink(&self) -> &Si {
            &self.sink
        }
        pub fn stream_mut(&mut self) -> &mut St {
            &mut self.stream
        }

        pub fn sink_mut(&mut self) -> &mut Si {
            &mut self.sink
        }
    }

    impl<Si, St, StreamItem> Stream for SinkStream<Si, St>
    where
        St: Stream<Item = StreamItem> + Unpin,
    {
        type Item = StreamItem;

        fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
            self.project().stream.poll_next(cx)
        }
    }

    impl<Si, St, SinkItem, Error> Sink<SinkItem> for SinkStream<Si, St>
    where
        Si: Sink<SinkItem, Error = Error> + Unpin,
    {
        type Error = Error;

        fn poll_ready(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
            self.project().sink.poll_ready(cx)
        }

        fn start_send(self: Pin<&mut Self>, item: SinkItem) -> Result<(), Self::Error> {
            self.project().sink.start_send(item)
        }

        fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
            self.project().sink.poll_flush(cx)
        }

        fn poll_close(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
            self.project().sink.poll_close(cx)
        }
    }

    #[pin_project]
    struct TestStream<T> {
        items: VecDeque<T>,
    }

    impl<T> Stream for TestStream<T> {
        type Item = T;
        fn poll_next(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
            let item = self.project().items.pop_front();
            Poll::Ready(item)
        }
    }

    impl<T> TestStream<T> {
        pub fn new() -> TestStream<T> {
            TestStream {
                items: VecDeque::new(),
            }
        }

        pub fn _items<'a>(&'a self) -> std::collections::vec_deque::Iter<'a, T> {
            self.items.iter()
        }

        pub fn add_item(&mut self, item: T) {
            self.items.push_back(item);
        }
    }

    #[pin_project]
    struct TestSink<T> {
        items: VecDeque<T>,
    }

    impl<T> Sink<T> for TestSink<T> {
        type Error = ();

        fn poll_ready(
            self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
        ) -> Poll<Result<(), Self::Error>> {
            Poll::Ready(Ok(()))
        }

        fn start_send(self: Pin<&mut Self>, item: T) -> Result<(), Self::Error> {
            self.project().items.push_back(item);
            Ok(())
        }

        fn poll_flush(
            self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
        ) -> Poll<Result<(), Self::Error>> {
            Poll::Ready(Ok(()))
        }

        fn poll_close(
            self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
        ) -> Poll<Result<(), Self::Error>> {
            Poll::Ready(Ok(()))
        }
    }

    impl<T> TestSink<T> {
        pub fn new() -> TestSink<T> {
            TestSink {
                items: VecDeque::new(),
            }
        }

        fn items<'a>(&'a self) -> std::collections::vec_deque::Iter<'a, T> {
            self.items.iter()
        }

        fn clear(&mut self) {
            self.items.clear();
        }
    }

    trait PeerLink<T> {
        fn link(&mut self) -> &mut T;
    }

    impl<T> PeerLink<T> for Option<&mut LinkedPeer<T>> {
        fn link(&mut self) -> &mut T {
            self.as_mut().unwrap().link.as_mut().unwrap()
        }
    }

    impl<T> PeerLink<T> for LinkedPeer<T> {
        fn link(&mut self) -> &mut T {
            self.link.as_mut().unwrap()
        }
    }

    fn new_peer(
        address: &str,
    ) -> (
        LinkedPeer<SinkStream<TestSink<MuMessage>, TestStream<Result<MuMessage, ()>>>>,
        NodeHash,
    ) {
        let other_node = Node::new(address, 1234);

        let link = SinkStream::new(TestSink::new(), TestStream::<Result<_, ()>>::new());

        (
            LinkedPeer::new(
                other_node.clone(),
                vec![other_node.hash.clone()],
                Some(link),
            ),
            other_node.hash,
        )
    }

    #[tokio::test]
    async fn test_proc_incoming_conn() {
        let my_node = Node::new("test", 1234);
        let mut state = GossipState::new(my_node.clone(), Default::default());

        let (mut peer, hash) = new_peer("address_other");

        let peer_node = peer.node().clone();
        peer.link()
            .stream_mut()
            .add_item(Ok::<_, ()>(MuMessage::Hello(peer_node)));

        state.proc_incoming_conn(peer.link()).await;

        assert_eq!(state.peers.keys().collect::<Vec<_>>(), vec![&hash]);
        assert_eq!(
            peer.link().sink().items().collect::<Vec<_>>(),
            vec![&MuMessage::Hello(my_node)],
        );
        println!("{:?}", peer.link.as_mut().unwrap().sink().items());
    }

    #[tokio::test]
    async fn test_proc_heartbeat_from_peer() {
        let mut state = GossipState::new(Node::new("test", 1), Default::default());

        let (peer1, hash1) = new_peer("peer1");
        let (peer2, hash2) = new_peer("peer2");
        let (peer3, hash3) = new_peer("peer3");

        let heartbeat = MuMessage::Heartbeat(Heartbeat {
            node: peer1.node().clone(),
            seq: 1,
        });

        state.peers.insert(hash1.clone(), peer1);
        state.peers.insert(hash2.clone(), peer2);
        state.peers.insert(hash3.clone(), peer3);

        state.proc_message(&hash1, heartbeat.clone()).await;

        //It should update the last_heartbeat field
        assert_eq!(state.peers.get(&hash1).unwrap().last_heartbeat(), 1);

        //It should forward the heartbeat to all peers, except the one it received the heartbeat from
        assert_eq!(state.peers.get_mut(&hash1).link().sink().items().count(), 0);
        assert_eq!(
            state
                .peers
                .get_mut(&hash2)
                .link()
                .sink()
                .items()
                .collect::<Vec<_>>(),
            vec![&heartbeat]
        );
        assert_eq!(
            state
                .peers
                .get_mut(&hash3)
                .link()
                .sink()
                .items()
                .collect::<Vec<_>>(),
            vec![&heartbeat]
        );

        // It should not resend the heartbeat if it's seen it before
        state.peers.get_mut(&hash1).link().sink_mut().clear();
        state.peers.get_mut(&hash2).link().sink_mut().clear();
        state.peers.get_mut(&hash3).link().sink_mut().clear();

        state.proc_message(&hash1, heartbeat.clone()).await;

        assert_eq!(state.peers.get_mut(&hash1).link().sink().items().count(), 0,);
        assert_eq!(state.peers.get_mut(&hash2).link().sink().items().count(), 0,);
        assert_eq!(state.peers.get_mut(&hash3).link().sink().items().count(), 0,);
    }

    #[tokio::test]
    async fn test_proc_heartbeat_from_non_peer() {
        let mut state = GossipState::new(Node::new("test", 1), Default::default());
        let (peer1, hash1) = new_peer("peer1");
        let (peer2, hash2) = new_peer("peer2");
        let (peer3, hash3) = new_peer("peer3");

        let heartbeat = MuMessage::Heartbeat(Heartbeat {
            node: peer2.node().clone(),
            seq: 1,
        });

        state.peers.insert(hash1.clone(), peer1);
        state.peers.insert(hash3.clone(), peer3);

        state.proc_message(&hash1, heartbeat.clone()).await;

        // It should update the last_heartbeat field
        let peer2 = state.peers.get(&hash2).unwrap();
        assert_eq!(peer2.last_heartbeat(), 1);
        assert_eq!(peer2.seen_from().iter().collect::<Vec<_>>(), vec![&hash1]);
        assert!(peer2.link.is_none());

        // It should forward the message to its other peers but not the one it received it from
        assert_eq!(
            state
                .peers
                .get_mut(&hash3)
                .link()
                .sink()
                .items()
                .collect::<Vec<_>>(),
            vec![&heartbeat]
        );

        assert_eq!(state.peers.get_mut(&hash1).link().sink().items().count(), 0);

        // It should not resend the heartbeat if it's seen it before
        state.peers.get_mut(&hash1).link().sink_mut().clear();
        state.peers.get_mut(&hash3).link().sink_mut().clear();

        state.proc_message(&hash1, heartbeat.clone()).await;

        assert_eq!(state.peers.get_mut(&hash1).link().sink().items().count(), 0,);
        assert_eq!(state.peers.get_mut(&hash3).link().sink().items().count(), 0,);
    }
}
