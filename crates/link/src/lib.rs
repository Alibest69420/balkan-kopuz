//! The one seam to a Kopuz daemon: a gRPC client on its own tokio runtime, supervised as a GPUI entity.

mod daemon_process;
mod socket;

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::task::Poll;
use std::time::Duration;

use api::{ApiError, ApiEvent, ErrorCode, KopuzApi};
use futures::StreamExt as _;
use gpui_kit::{App, AppContext as _, Context, Entity, EventEmitter, Global, Task};

pub use api;
pub use client::Address;
pub use daemon_process::spawn_daemon;
pub use socket::default_socket_path;

const RETRY_INTERVAL: Duration = Duration::from_secs(2);

/// The daemon client plus the runtime its futures run on. Cheap to clone.
#[derive(Clone)]
pub struct Link {
    api: Arc<dyn KopuzApi>,
    runtime: tokio::runtime::Handle,
    address: Address,
}

struct GlobalLink {
    link: Link,
    connection: Entity<Connection>,
    // Dropping the runtime would cancel every call in flight.
    _runtime: Arc<tokio::runtime::Runtime>,
}

impl Global for GlobalLink {}

impl Link {
    /// The link for the running app; panics before [`init`].
    pub fn global(cx: &App) -> &Link {
        &cx.global::<GlobalLink>().link
    }

    pub fn connection(cx: &App) -> Entity<Connection> {
        cx.global::<GlobalLink>().connection.clone()
    }

    pub fn address(&self) -> &Address {
        &self.address
    }

    /// Run `f` on the link's runtime; the returned future is awaitable from a GPUI task.
    pub fn call<T, Fut>(&self, f: impl FnOnce(Arc<dyn KopuzApi>) -> Fut) -> Call<T>
    where
        T: Send + 'static,
        Fut: Future<Output = Result<T, ApiError>> + Send + 'static,
    {
        Call(self.runtime.spawn(f(self.api.clone())))
    }

    /// Fire a command whose effect arrives on the event stream; failure is only logged.
    pub fn fire<T, Fut>(
        &self,
        cx: &App,
        what: &'static str,
        f: impl FnOnce(Arc<dyn KopuzApi>) -> Fut,
    ) where
        T: Send + 'static,
        Fut: Future<Output = Result<T, ApiError>> + Send + 'static,
    {
        let call = self.call(f);
        cx.background_spawn(async move {
            if let Err(error) = call.await {
                tracing::warn!(%error, what, "daemon command failed");
            }
        })
        .detach();
    }
}

/// A daemon call in flight on the link's runtime.
pub struct Call<T>(tokio::task::JoinHandle<Result<T, ApiError>>);

impl<T> Future for Call<T> {
    type Output = Result<T, ApiError>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> Poll<Self::Output> {
        Pin::new(&mut self.0).poll(cx).map(|joined| {
            joined.unwrap_or_else(|error| {
                Err(ApiError {
                    code: ErrorCode::Internal,
                    message: format!("daemon call aborted: {error}"),
                })
            })
        })
    }
}

/// Connect to the daemon at `address` and start supervising it; call once before any window opens.
pub fn init(address: Address, cx: &mut App) -> anyhow::Result<()> {
    let runtime = Arc::new(
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .thread_name("kopuz-link")
            .enable_all()
            .build()?,
    );
    let api: Arc<dyn KopuzApi> = {
        let _guard = runtime.enter();
        match &address {
            Address::Socket(path) => Arc::new(client::GrpcApi::new(path.clone())?),
            Address::Tcp(host) => {
                let token = std::env::var("KOPUZ_DAEMON_TOKEN").unwrap_or_default();
                Arc::new(client::GrpcApi::connect_tcp(host.clone(), &token)?)
            }
        }
    };
    let link = Link {
        api,
        runtime: runtime.handle().clone(),
        address,
    };
    let connection = cx.new(|cx| Connection::new(link.clone(), cx));
    cx.set_global(GlobalLink {
        link,
        connection,
        _runtime: runtime,
    });
    Ok(())
}

#[derive(Debug, Clone, PartialEq)]
pub enum ConnectionStatus {
    Connecting,
    Connected,
    /// The last attempt failed; another follows after [`RETRY_INTERVAL`].
    Unreachable(ApiError),
}

/// Supervises the daemon socket and relays its event stream.
pub struct Connection {
    status: ConnectionStatus,
    /// Failed probes since the last time the daemon answered, for the banner's "attempt 3".
    attempts: u32,
    retry: futures::channel::mpsc::UnboundedSender<()>,
    link: Link,
    _supervisor: Task<()>,
}

impl EventEmitter<ApiEvent> for Connection {}

impl Connection {
    fn new(link: Link, cx: &mut Context<Self>) -> Self {
        let (retry, mut retries) = futures::channel::mpsc::unbounded();
        let supervisor = cx.spawn({
            let link = link.clone();
            async move |this, cx| {
                loop {
                    let probe = link.call(|api| async move { api.player_state().await });
                    if let Err(error) = probe.await {
                        let unreachable = ConnectionStatus::Unreachable(error);
                        if this
                            .update(cx, |this, cx| {
                                this.attempts = this.attempts.saturating_add(1);
                                this.set_status(unreachable, cx);
                                cx.notify();
                            })
                            .is_err()
                        {
                            return;
                        }
                        let timer = cx.background_executor().timer(RETRY_INTERVAL);
                        // "Retry now" is the same wait, cut short.
                        futures::future::select(Box::pin(timer), Box::pin(retries.next())).await;
                        continue;
                    }

                    let mut events = relay_events(&link);
                    let attached = this.update(cx, |this, cx| {
                        this.attempts = 0;
                        this.set_status(ConnectionStatus::Connected, cx);
                        cx.emit(ApiEvent::Resync);
                    });
                    if attached.is_err() {
                        return;
                    }
                    while let Some(event) = events.next().await {
                        if this.update(cx, |_, cx| cx.emit(event)).is_err() {
                            return;
                        }
                    }
                    tracing::info!("daemon event stream ended");
                    let lost = ConnectionStatus::Connecting;
                    if this
                        .update(cx, |this, cx| this.set_status(lost, cx))
                        .is_err()
                    {
                        return;
                    }
                }
            }
        });
        Self {
            status: ConnectionStatus::Connecting,
            attempts: 0,
            retry,
            link,
            _supervisor: supervisor,
        }
    }

    pub fn status(&self) -> &ConnectionStatus {
        &self.status
    }

    pub fn attempts(&self) -> u32 {
        self.attempts
    }

    /// Stop waiting out the retry interval and probe the daemon again now.
    pub fn retry_now(&self) {
        let _ = self.retry.unbounded_send(());
    }

    pub fn is_connected(&self) -> bool {
        self.status == ConnectionStatus::Connected
    }

    pub fn link(&self) -> &Link {
        &self.link
    }

    fn set_status(&mut self, status: ConnectionStatus, cx: &mut Context<Self>) {
        if self.status != status {
            self.status = status;
            cx.notify();
        }
    }
}

/// Subscribe on the link's runtime and forward into a channel GPUI can poll.
fn relay_events(link: &Link) -> futures::channel::mpsc::UnboundedReceiver<ApiEvent> {
    let (tx, rx) = futures::channel::mpsc::unbounded();
    let api = link.api.clone();
    link.runtime.spawn(async move {
        let mut stream = api.events();
        while let Some(event) = stream.next().await {
            if tx.unbounded_send(event).is_err() {
                return;
            }
        }
    });
    rx
}
