// src/connection.rs
use crate::imports::*;
use std::fmt;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

// workflow_core time utilities (for elapsed time only)
use workflow_core::time::Instant;

/// Returns a ready-to-print UTC timestamp like `2025-10-30T12:34:56.789Z`
/// using only `std::time::SystemTime` (no chrono needed)
fn timestamp() -> String {
    let now = SystemTime::now();
    let duration = now.duration_since(UNIX_EPOCH).expect("Time went backwards");
    let secs = duration.as_secs();
    let nanos = duration.subsec_nanos();

    // Manual UTC formatting: YYYY-MM-DDTHH:MM:SS.mmmZ
    let (year, month, day, hour, min, sec) = {
        // Simple but accurate enough for logging (no leap seconds)
        let days_since_epoch = secs / 86_400;
        let secs_in_day = secs % 86_400;
        let hour = secs_in_day / 3600;
        let min = (secs_in_day % 3600) / 60;
        let sec = secs_in_day % 60;

        // Approximate Gregorian calendar (good for 2020–2100)
        let mut y = 1970;
        let mut d = days_since_epoch as i64;
        while d >= 365 { 
            if (y % 4 == 0 && y % 100 != 0) || y % 400 == 0 { d -= 366; } else { d -= 365; }
            y += 1;
        }
        let mut m = 1;
        let day;
        loop {
            let days_in_month = match m {
                4 | 6 | 9 | 11 => 30,
                2 => if (y % 4 == 0 && y % 100 != 0) || y % 400 == 0 { 29 } else { 28 },
                _ => 31,
            };
            if d < days_in_month as i64 { day = d as u32 + 1; break; }
            d -= days_in_month as i64;
            m += 1;
        }
        (y, m, day, hour as u32, min as u32, sec as u32)
    };

    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:03}Z",
        year, month, day, hour, min, sec, nanos / 1_000_000
    )
}

impl fmt::Display for Connection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let load = self
            .load()
            .map(|load| format!("{:1.2}%", load * 100.0))
            .unwrap_or_else(|| "n/a  ".to_string());
        write!(
            f,
            "[{:016x}:{:016x}] [{:>4}] [{:>7}] {}",
            self.system_id(),
            self.node.uid(),
            self.clients(),
            load,
            self.node.address
        )
    }
}

#[derive(Debug)]
pub struct Connection {
    args: Arc<Args>,
    caps: ArcSwapOption<Caps>,
    is_synced: AtomicBool,
    clients: AtomicU64,
    peers: AtomicU64,
    node: Arc<Node>,
    monitor: Arc<Monitor>,
    params: PathParams,
    client: rpc::Client,
    shutdown_ctl: DuplexChannel<()>,
    delegate: ArcSwap<Option<Arc<Connection>>>,
    is_connected: AtomicBool,
    is_online: AtomicBool,
}

impl Connection {
    pub fn try_new(
        monitor: Arc<Monitor>,
        node: Arc<Node>,
        _sender: Sender<PathParams>,
        args: &Arc<Args>,
    ) -> Result<Self> {
        let params = *node.params();

        let client = match node.transport_kind {
            TransportKind::WrpcBorsh => {
                rpc::vecno::Client::try_new(WrpcEncoding::Borsh, &node.address)?
            }
            TransportKind::WrpcJson => {
                rpc::vecno::Client::try_new(WrpcEncoding::SerdeJson, &node.address)?
            }
            TransportKind::Grpc => {
                unimplemented!("gRPC support is not currently implemented")
            }
        };

        let client = rpc::Client::from(client);

        Ok(Self {
            args: args.clone(),
            caps: ArcSwapOption::new(None),
            monitor,
            params,
            node,
            client,
            shutdown_ctl: DuplexChannel::oneshot(),
            delegate: ArcSwap::new(Arc::new(None)),
            is_connected: AtomicBool::new(false),
            is_synced: AtomicBool::new(false),
            clients: AtomicU64::new(0),
            peers: AtomicU64::new(0),
            is_online: AtomicBool::new(false),
        })
    }

    #[inline] pub fn verbose(&self) -> bool { self.args.verbose }
    #[inline] pub fn score(self: &Arc<Self>) -> u64 { self.delegate().sockets() }

    #[inline]
    pub fn is_available(self: &Arc<Self>) -> bool {
        let delegate = self.delegate();
        self.is_connected()
            && delegate.is_online()
            && delegate.caps.load().as_ref().as_ref().is_some_and(|caps| {
                let clients = delegate.clients();
                let peers = delegate.peers();
                clients < caps.clients_limit && clients + peers < caps.fd_limit
            })
    }

    #[inline] pub fn is_connected(&self) -> bool { self.is_connected.load(Ordering::Relaxed) }
    #[inline] pub fn is_online(&self) -> bool { self.is_online.load(Ordering::Relaxed) }
    #[inline] pub fn is_synced(&self) -> bool { self.is_synced.load(Ordering::Relaxed) }
    #[inline] pub fn clients(&self) -> u64 { self.clients.load(Ordering::Relaxed) }
    #[inline] pub fn peers(&self) -> u64 { self.peers.load(Ordering::Relaxed) }
    #[inline] pub fn sockets(&self) -> u64 { self.clients() + self.peers() }

    pub fn load(&self) -> Option<f64> {
        self.caps.load().as_ref().map(|caps| self.clients() as f64 / caps.capacity as f64)
    }

    #[inline] pub fn caps(&self) -> Option<Arc<Caps>> { self.caps.load().clone() }
    #[inline] pub fn system_id(&self) -> u64 {
        self.caps.load().as_ref().map(|c| c.system_id).unwrap_or_default()
    }
    #[inline] pub fn address(&self) -> &str { self.node.address.as_str() }
    #[inline] pub fn node(&self) -> &Arc<Node> { &self.node }
    #[inline] pub fn params(&self) -> PathParams { self.params }
    #[inline] pub fn network_id(&self) -> NetworkId { self.node.network }
    #[inline] pub fn is_delegate(&self) -> bool { self.delegate.load().is_none() }

    #[inline]
    pub fn delegate(self: &Arc<Self>) -> Arc<Connection> {
        match (**self.delegate.load()).clone() {
            Some(d) => d.delegate(),
            None => self.clone(),
        }
    }

    #[inline]
    pub fn bind_delegate(&self, delegate: Option<Arc<Connection>>) {
        self.delegate.store(Arc::new(delegate));
    }

    pub fn resolve_delegators(self: &Arc<Self>) -> Vec<Arc<Connection>> {
        let mut list = Vec::new();
        let mut current = (*self).clone();
        while let Some(next) = (**current.delegate.load()).clone() {
            list.push(next.clone());
            current = next;
        }
        list
    }

    pub fn status(&self) -> &'static str {
        if self.is_connected() {
            if !self.is_delegate() {
                "delegator"
            } else if self.is_synced() {
                "online"
            } else {
                "syncing"
            }
        } else {
            "offline"
        }
    }

    async fn connect(&self) -> Result<()> {
        self.client.connect().await?;
        Ok(())
    }

    /// Hybrid reset: graceful disconnect to trigger_abort fallback
    async fn hard_reset(&self) -> Result<()> {
        if self.is_connected.load(Ordering::Relaxed) {
            match self.client.disconnect().await {
                Ok(()) => {
                    let ts = timestamp();
                    log_info!("Reset", "[{ts}] graceful disconnect");
                }
                Err(_) => {
                    let ts = timestamp();
                    log_warn!("Reset", "[{ts}] graceful failed to hard abort");
                    let _ = self.client.trigger_abort();
                }
            }
        }
        self.caps.store(None);
        self.client.connect().await
    }

    pub async fn task(self: Arc<Self>) -> Result<()> {
        self.connect().await?;
        let rpc_ctl_channel = self.client.multiplexer().channel();
        let shutdown_ctl_receiver = self.shutdown_ctl.request.receiver.clone();
        let shutdown_ctl_sender = self.shutdown_ctl.response.sender.clone();

        let mut ttl = TtlSettings::ttl();
        let mut poll = if self.is_delegate() {
            interval(SyncSettings::poll())
        } else {
            interval(SyncSettings::ping())
        };

        let mut last_connect_time: Option<Instant> = None;

        loop {
            select! {
                _ = poll.next().fuse() => {
                    if TtlSettings::enable() {
                        if let Some(t) = last_connect_time {
                            if t.elapsed() > ttl {
                                last_connect_time = None;
                                let _ = self.hard_reset().await;
                                continue;
                            }
                        }
                    }

                    if self.is_connected.load(Ordering::Relaxed) {
                        let was_online = self.is_online.load(Ordering::Relaxed);
                        let is_online = self.update_state().await.is_ok();
                        self.is_online.store(is_online, Ordering::Relaxed);

                        if is_online != was_online {
                            let ts = timestamp();
                            if is_online {
                                log_success!("Online", "[{ts}] {}", self.node.address);
                            } else {
                                log_error!("Offline", "[{ts}] {}", self.node.address);
                            }
                            self.update();
                        }
                    }
                }

                msg = rpc_ctl_channel.receiver.recv().fuse() => {
                    match msg {
                        Ok(Ctl::Connect) => {
                            last_connect_time = Some(Instant::now());
                            ttl = TtlSettings::ttl();
                            let ts = timestamp();

                            if self.args.verbose {
                                log_info!(
                                    "Connected",
                                    "[{ts}] {} - ttl: {:.2}h",
                                    self.node.address,
                                    ttl.as_secs_f64() / 3600.0
                                );
                            } else {
                                log_success!("Connected", "[{ts}] {}", self.node.address);
                            }

                            self.is_connected.store(true, Ordering::Relaxed);

                            if self.caps().is_some() {
                                let _ = self.update_caps().await;
                            }

                            if self.update_state().await.is_ok() {
                                self.is_online.store(true, Ordering::Relaxed);
                            } else {
                                self.is_online.store(false, Ordering::Relaxed);
                            }
                            self.update();
                        }

                        Ok(Ctl::Disconnect) => {
                            self.is_connected.store(false, Ordering::Relaxed);
                            self.is_online.store(false, Ordering::Relaxed);
                            last_connect_time = None;
                            self.update();
                            let ts = timestamp();
                            log_error!("Disconnected", "[{ts}] {}", self.node.address);
                        }

                        Err(err) => {
                            let ts = timestamp();
                            log_error!("Monitor", "[{ts}] rpc_ctl_channel error: {err}");
                            break;
                        }
                    }
                }

                _ = shutdown_ctl_receiver.recv().fuse() => break,
            }
        }

        shutdown_ctl_sender.send(()).await.unwrap();
        Ok(())
    }

    pub fn start(self: &Arc<Self>) -> Result<()> {
        let this = self.clone();
        spawn(async move {
            if let Err(e) = this.task().await {
                let ts = timestamp();
                log_error!("Task", "[{ts}] NodeConnection error: {:?}", e);
            }
        });
        Ok(())
    }

    pub async fn stop(self: &Arc<Self>) -> Result<()> {
        self.shutdown_ctl.signal(()).await.expect("shutdown signal failed");
        Ok(())
    }

    async fn update_caps(self: &Arc<Self>) -> Result<()> {
        if let Some(prev) = self.caps().as_ref() {
            let new = self.client.get_caps().await?;
            let caps = Caps::with_version(prev, new.version);
            self.caps.store(Some(Arc::new(caps)));
        }
        Ok(())
    }

    async fn update_state(self: &Arc<Self>) -> Result<()> {
        if !self.is_delegate() {
            let _ = self.client.ping().await;
            return Ok(());
        }

        if self.caps().is_none() {
            let last_id = self.caps().as_ref().map(|c| c.system_id());
            let caps = self.client.get_caps().await?;
            let sys_id = caps.system_id();
            self.caps.store(Some(Arc::new(caps)));

            if last_id != Some(sys_id) {
                let key = Delegate::new(sys_id, self.network_id());
                let mut map = self.monitor.delegates().write().unwrap();
                if let Some(existing) = map.get(&key) {
                    self.bind_delegate(Some(existing.clone()));
                } else {
                    map.insert(key, self.clone());
                    self.bind_delegate(None);
                }
            }
        }

        match self.client.get_sync().await {
            Ok(sync) => {
                let was_sync = self.is_synced.load(Ordering::Relaxed);
                self.is_synced.store(sync, Ordering::Relaxed);

                if sync {
                    match self.client.get_active_connections().await {
                        Ok(Connections { clients, peers }) => {
                            let pc = self.clients.load(Ordering::Relaxed);
                            let pp = self.peers.load(Ordering::Relaxed);

                            self.clients.store(clients, Ordering::Relaxed);
                            self.peers.store(peers, Ordering::Relaxed);

                            if self.verbose() && (clients != pc || peers != pp) {
                                let ts = timestamp();
                                log_success!("Clients", "[{ts}] {self}");
                            }
                            Ok(())
                        }
                        Err(e) => {
                            let ts = timestamp();
                            log_error!("RPC", "[{ts}] {self}");
                            log_error!("Error", "[{ts}] {e}");
                            Err(Error::Metrics)
                        }
                    }
                } else {
                    if sync != was_sync {
                        let ts = timestamp();
                        log_error!("Sync", "[{ts}] {self}");
                    }
                    Err(Error::Sync)
                }
            }
            Err(e) => {
                let ts = timestamp();
                log_error!("RPC", "[{ts}] {self}");
                log_error!("Error", "[{ts}] {e}");
                Err(Error::Status)
            }
        }
    }

    #[inline]
    pub fn update(&self) {
        self.monitor.schedule_sort(&self.params);
    }
}

#[derive(Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct Output<'a> {
    pub uid: &'a str,
    pub url: &'a str,
}

impl<'a> From<&'a Arc<Connection>> for Output<'a> {
    fn from(conn: &'a Arc<Connection>) -> Self {
        Self {
            uid: conn.node.uid_as_str(),
            url: conn.node.address(),
        }
    }
}