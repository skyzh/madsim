use super::{server::Request, KeyValue, ResponseHeader, Result};
use futures_util::stream::{Stream, StreamExt};
use madsim::net::{Endpoint, Receiver};
use std::{
    net::SocketAddr,
    pin::Pin,
    task::{Context, Poll},
};

/// Client for Elect operations.
#[derive(Clone)]
pub struct ElectionClient {
    ep: Endpoint,
    server_addr: SocketAddr,
}

impl ElectionClient {
    /// Create a new [`ElectionClient`].
    pub(crate) fn new(ep: Endpoint) -> Self {
        ElectionClient {
            server_addr: ep.peer_addr().unwrap(),
            ep,
        }
    }

    /// Puts a value as eligible for the election on the prefix key.
    /// Multiple sessions can participate in the election for the
    /// same prefix, but only one can be the leader at a time.
    #[inline]
    pub async fn campaign(
        &mut self,
        name: impl Into<Vec<u8>>,
        value: impl Into<Vec<u8>>,
        lease: i64,
    ) -> Result<CampaignResponse> {
        let req = Request::Campaign {
            name: name.into(),
            value: value.into(),
            lease,
        };
        let (tx, mut rx) = self.ep.connect1(self.server_addr).await?;
        tx.send(Box::new(req)).await?;
        *rx.recv().await?.downcast().unwrap()
    }

    /// Lets the leader announce a new value without another election.
    #[inline]
    pub async fn proclaim(
        &mut self,
        value: impl Into<Vec<u8>>,
        options: Option<ProclaimOptions>,
    ) -> Result<ProclaimResponse> {
        let req = Request::Proclaim {
            leader: options
                .expect("no leader key")
                .leader
                .expect("no leader key"),
            value: value.into(),
        };
        let (tx, mut rx) = self.ep.connect1(self.server_addr).await?;
        tx.send(Box::new(req)).await?;
        *rx.recv().await?.downcast().unwrap()
    }

    /// Returns the leader value for the current election.
    #[inline]
    pub async fn leader(&mut self, name: impl Into<Vec<u8>>) -> Result<LeaderResponse> {
        let req = Request::Leader { name: name.into() };
        let (tx, mut rx) = self.ep.connect1(self.server_addr).await?;
        tx.send(Box::new(req)).await?;
        *rx.recv().await?.downcast().unwrap()
    }

    /// Returns a channel that reliably observes ordered leader proposals
    /// as GetResponse values on every current elected leader key.
    #[inline]
    pub async fn observe(&mut self, name: impl Into<Vec<u8>>) -> Result<ObserveStream> {
        let req = Request::Observe { name: name.into() };
        let (tx, rx) = self.ep.connect1(self.server_addr).await?;
        tx.send(Box::new(req)).await?;
        Ok(ObserveStream { rx })
    }

    /// Releases election leadership and then start a new election
    #[inline]
    pub async fn resign(&mut self, options: Option<ResignOptions>) -> Result<ResignResponse> {
        let req = Request::Resign {
            leader: options
                .expect("no leader key")
                .leader
                .expect("no leader key"),
        };
        let (tx, mut rx) = self.ep.connect1(self.server_addr).await?;
        tx.send(Box::new(req)).await?;
        *rx.recv().await?.downcast().unwrap()
    }
}

/// Response for `Campaign` operation.
#[derive(Debug, Clone)]
pub struct CampaignResponse {
    pub(crate) header: ResponseHeader,
    pub(crate) leader: LeaderKey,
}

impl CampaignResponse {
    /// Get response header.
    #[inline]
    pub fn header(&self) -> Option<&ResponseHeader> {
        Some(&self.header)
    }

    /// Describes the resources used for holding leadership of the election.
    #[inline]
    pub fn leader(&self) -> Option<&LeaderKey> {
        Some(&self.leader)
    }
}

/// Options for `proclaim` operation.
#[derive(Debug, Default, Clone)]
pub struct ProclaimOptions {
    leader: Option<LeaderKey>,
}

impl ProclaimOptions {
    #[inline]
    pub const fn new() -> Self {
        Self { leader: None }
    }

    /// The leadership hold on the election.
    #[inline]
    pub fn with_leader(mut self, leader: LeaderKey) -> Self {
        self.leader = Some(leader);
        self
    }
}

/// Leader key of election
#[derive(Debug, Clone)]
pub struct LeaderKey {
    pub(crate) name: Vec<u8>,
    pub(crate) key: Vec<u8>,
    pub(crate) rev: i64,
    pub(crate) lease: i64,
}

impl LeaderKey {
    /// Creates a new leader key.
    #[inline]
    pub const fn new() -> Self {
        Self {
            name: Vec::new(),
            key: Vec::new(),
            rev: 0,
            lease: 0,
        }
    }

    /// The election identifier that corresponds to the leadership key.
    #[inline]
    pub fn with_name(mut self, name: impl Into<Vec<u8>>) -> Self {
        self.name = name.into();
        self
    }

    /// An opaque key representing the ownership of the election.
    #[inline]
    pub fn with_key(mut self, key: impl Into<Vec<u8>>) -> Self {
        self.key = key.into();
        self
    }

    /// The creation revision of the key
    #[inline]
    pub const fn with_rev(mut self, rev: i64) -> Self {
        self.rev = rev;
        self
    }

    /// The lease ID of the election leader.
    #[inline]
    pub const fn with_lease(mut self, lease: i64) -> Self {
        self.lease = lease;
        self
    }

    /// The name in byte. name is the election identifier that corresponds to the leadership key.
    #[inline]
    pub fn name(&self) -> &[u8] {
        &self.name
    }

    /// The name in string. name is the election identifier that corresponds to the leadership key.
    #[inline]
    pub fn name_str(&self) -> Result<&str> {
        std::str::from_utf8(self.name()).map_err(From::from)
    }

    // /// The name in string. name is the election identifier that corresponds to the leadership key.
    // #[inline]
    // pub unsafe fn name_str_unchecked(&self) -> &str {
    //     std::str::from_utf8_unchecked(self.name())
    // }

    /// The key in byte. key is an opaque key representing the ownership of the election. If the key
    /// is deleted, then leadership is lost.
    #[inline]
    pub fn key(&self) -> &[u8] {
        &self.key
    }

    /// The key in string. key is an opaque key representing the ownership of the election. If the key
    /// is deleted, then leadership is lost.
    #[inline]
    pub fn key_str(&self) -> Result<&str> {
        std::str::from_utf8(self.key()).map_err(From::from)
    }

    // /// The key in string. key is an opaque key representing the ownership of the election. If the key
    // /// is deleted, then leadership is lost.
    // #[inline]
    // pub unsafe fn key_str_unchecked(&self) -> &str {
    //     std::str::from_utf8_unchecked(self.key())
    // }

    /// The creation revision of the key.  It can be used to test for ownership
    /// of an election during transactions by testing the key's creation revision
    /// matches rev.
    #[inline]
    pub const fn rev(&self) -> i64 {
        self.rev
    }

    /// The lease ID of the election leader.
    #[inline]
    pub const fn lease(&self) -> i64 {
        self.lease
    }
}

/// Response for `Proclaim` operation.
#[derive(Debug, Clone)]
pub struct ProclaimResponse {
    pub(crate) header: ResponseHeader,
}

impl ProclaimResponse {
    /// Gets response header.
    #[inline]
    pub fn header(&self) -> Option<&ResponseHeader> {
        Some(&self.header)
    }
}

/// Response for `Leader` operation.
#[derive(Debug, Clone)]
pub struct LeaderResponse {
    pub(crate) header: ResponseHeader,
    pub(crate) kv: Option<KeyValue>,
}

impl LeaderResponse {
    /// Gets response header.
    #[inline]
    pub fn header(&self) -> Option<&ResponseHeader> {
        Some(&self.header)
    }

    /// The key-value pair representing the latest leader update.
    #[inline]
    pub fn kv(&self) -> Option<&KeyValue> {
        self.kv.as_ref()
    }
}

/// Response for `Observe` operation.
#[derive(Debug)]
pub struct ObserveStream {
    rx: Receiver,
}

impl ObserveStream {
    /// Fetches the next message from this stream.
    #[inline]
    pub async fn message(&mut self) -> Result<Option<LeaderResponse>> {
        let rsp = *(self.rx.recv().await?)
            .downcast::<Result<LeaderResponse>>()
            .unwrap();
        rsp.map(Some)
    }
}

impl Stream for ObserveStream {
    type Item = Result<LeaderResponse>;

    #[inline]
    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        match self.rx.poll_next_unpin(cx) {
            Poll::Ready(Some(Ok(payload))) => {
                Poll::Ready(Some(*payload.downcast::<Result<LeaderResponse>>().unwrap()))
            }
            Poll::Ready(Some(Err(e))) => Poll::Ready(Some(Err(e.into()))),
            Poll::Ready(None) => Poll::Ready(None),
            Poll::Pending => Poll::Pending,
        }
    }
}

/// Options for `resign` operation.
#[derive(Debug, Default, Clone)]
pub struct ResignOptions {
    leader: Option<LeaderKey>,
}

impl ResignOptions {
    #[inline]
    pub const fn new() -> Self {
        Self { leader: None }
    }

    /// The leadership to relinquish by resignation.
    #[inline]
    pub fn with_leader(mut self, leader: LeaderKey) -> Self {
        self.leader = Some(leader);
        self
    }
}

/// Response for `Resign` operation.
#[derive(Debug, Clone)]
pub struct ResignResponse {
    pub(crate) header: ResponseHeader,
}

impl ResignResponse {
    /// Gets response header.
    #[inline]
    pub fn header(&self) -> Option<&ResponseHeader> {
        Some(&self.header)
    }
}
