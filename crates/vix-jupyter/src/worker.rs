//! The worker thread: owns the ZeroMQ sockets and the kernel process.

use std::path::PathBuf;
use std::process::Child;
use std::sync::mpsc::Sender;
use std::time::Duration;

use serde_json::Value;
use tokio::sync::mpsc::UnboundedReceiver;
use tokio::time::{Instant, interval, sleep};
use vix_jupyter_core::message::{ExecutionState, ReplyStatus, request};
use vix_jupyter_core::wire::{self, DecodeError};
use vix_jupyter_core::{
    Channel, ConnectionInfo, Content, KernelSpec, Limits, Message, Output, Signer,
};
use zeromq::{
    DealerSocket, Socket, SocketOptions, SocketRecv, SocketSend, SubSocket, ZmqMessage, ZmqResult,
};

use crate::event::{DeathReason, KernelEvent, KernelState, RequestId};
use crate::kernel::{LaunchOptions, StderrTail, spawn_kernel};
use crate::runtime_dir::RuntimeDir;

/// How often the worker checks whether the kernel process exited.
const EXIT_POLL: Duration = Duration::from_millis(100);

/// How long an `execute_reply` waits for its request's `idle` status on
/// IOPub (which marks the last output) before it is reported anyway, in
/// case IOPub dropped the status.
const REPLY_WAIT: Duration = Duration::from_secs(1);

/// Most requests tracked while their reply and `idle` status meet up.
const MAX_TRACKED: usize = 256;

/// A reply held back until IOPub's `idle` for the same request.
struct HeldReply {
    /// The `execute_request` this reply answers.
    request: String,
    /// How the execution ended.
    status: ReplyStatus,
    /// The kernel's `In[n]` counter, if it reported one.
    execution_count: Option<u64>,
    /// When the reply arrived, to give up waiting for `idle` after `REPLY_WAIT`.
    since: Instant,
}

/// A request from the handle.
#[derive(Debug)]
pub(crate) enum Command {
    /// Run `code`; `id` was handed to the caller already and tags every resulting event.
    Execute { id: String, code: String },
    /// Interrupt the running request.
    Interrupt,
    /// Restart the kernel on the same connection.
    Restart,
    /// Ask the kernel to exit, killing it after the grace period.
    Shutdown,
    /// Stop now without asking (used when the handle is dropped).
    Kill,
}

/// A kernel process Vix started. Dropping it kills the process, then
/// removes the runtime directory (fields drop in order, after `drop`).
pub(crate) struct Launched {
    /// The kernel process.
    pub(crate) child: Child,
    /// The last bytes of the kernel's stderr, reported if it dies.
    pub(crate) stderr: StderrTail,
    /// The kernelspec it was started from, kept so a restart can relaunch it.
    pub(crate) spec: KernelSpec,
    /// Working directory for the process, kept for relaunch.
    pub(crate) cwd: Option<PathBuf>,
    /// The private directory holding the connection file and IPC sockets. Declared last so it is removed only after the process has been killed.
    pub(crate) dir: RuntimeDir,
}

impl Drop for Launched {
    /// Kill the child only if it is still running, then always reap it (`wait`) so it cannot linger as a zombie.
    fn drop(&mut self) {
        if matches!(self.child.try_wait(), Ok(None)) {
            let _ = self.child.kill();
        }
        let _ = self.child.wait();
    }
}

impl Launched {
    /// `Some(reason)` once the child has exited (with its exit code and the tail of its stderr); `None` while it is still running or if it is not ours to watch.
    fn exited(&mut self) -> Option<DeathReason> {
        match self.child.try_wait() {
            Ok(Some(status)) => Some(DeathReason::Exited {
                code: status.code(),
                stderr: self.stderr.text(),
            }),
            Ok(None) => None,
            Err(e) => Some(DeathReason::Transport(e.to_string())),
        }
    }
}

/// The three ZeroMQ sockets the worker uses: `shell` and `control` request/reply (dealers) and `iopub` the broadcast subscription.
struct Sockets {
    /// Shell channel: execute and kernel_info requests and replies.
    shell: DealerSocket,
    /// Control channel: interrupt and shutdown.
    control: DealerSocket,
    /// IOPub channel: status and outputs, broadcast to all clients.
    iopub: SubSocket,
}

/// What to do when a connection ends: reconnect after a restart, or stop for good.
enum Next {
    /// Reconnect to a freshly relaunched kernel.
    Restart,
    /// Stop for good and report why.
    Stop(DeathReason),
}

/// State owned by the single worker task: the connection, the HMAC signer, the per-session ids, protocol limits and timeouts, the launched process (if any), the outgoing event channel, and the bookkeeping that pairs each `execute_reply` with its `idle` status.
pub(crate) struct Worker {
    /// Addresses and key of the kernel's channels.
    info: ConnectionInfo,
    /// Signs outgoing and verifies incoming messages.
    signer: Signer,
    /// Session id stamped into every message header.
    session: String,
    /// The `username` stamped into message headers.
    username: String,
    /// Size caps applied to received messages.
    limits: Limits,
    /// How long the kernel has to become ready.
    start_timeout: Duration,
    /// How long a kernel gets to exit after `shutdown_request`.
    grace: Duration,
    /// The process we started; `None` when attached to someone else's kernel.
    launched: Option<Launched>,
    /// Channel back to the [`KernelHandle`](crate::KernelHandle).
    events: Sender<KernelEvent>,
    /// Commands received while (re)connecting, replayed once connected.
    pending: Vec<Command>,
    /// Counter for worker-originated message ids.
    counter: u64,
    /// Replies that arrived before their request's `idle` status.
    held: Vec<HeldReply>,
    /// Requests whose `idle` status arrived before their reply.
    idle: Vec<String>,
}

/// Connect the shell, control and iopub sockets, each with the connect timeout. iopub subscribes to every topic (empty prefix); which outputs matter is decided later from each message's parent header.
async fn connect_sockets(
    shell: String,
    control: String,
    iopub: String,
    timeout: Duration,
) -> ZmqResult<Sockets> {
    let options = || {
        let mut o = SocketOptions::default();
        o.connect_timeout(timeout);
        o
    };
    let mut sub = SubSocket::with_options(options());
    sub.subscribe("").await?;
    sub.connect(&iopub).await?;
    let mut shell_socket = DealerSocket::with_options(options());
    shell_socket.connect(&shell).await?;
    let mut control_socket = DealerSocket::with_options(options());
    control_socket.connect(&control).await?;
    Ok(Sockets {
        shell: shell_socket,
        control: control_socket,
        iopub: sub,
    })
}

/// Convert wire frames to a multipart ZeroMQ message.
fn to_zmq(frames: Vec<Vec<u8>>) -> ZmqMessage {
    let mut frames = frames.into_iter();
    let mut message = ZmqMessage::from(frames.next().unwrap_or_default());
    for frame in frames {
        message.push_back(frame.into());
    }
    message
}

impl Worker {
    /// Create a worker for one connection; `launched` is `None` when attaching to a kernel someone else started.
    pub(crate) fn new(
        info: ConnectionInfo,
        signer: Signer,
        session: String,
        options: &LaunchOptions,
        launched: Option<Launched>,
        events: Sender<KernelEvent>,
    ) -> Self {
        Self {
            info,
            signer,
            session,
            username: options.username.clone(),
            limits: options.limits,
            start_timeout: options.start_timeout,
            grace: options.shutdown_grace,
            launched,
            events,
            pending: Vec::new(),
            counter: 0,
            held: Vec::new(),
            idle: Vec::new(),
        }
    }

    /// Send an event to the App; a dropped receiver is ignored because it just means nobody is listening any more.
    fn emit(&self, event: KernelEvent) {
        let _ = self.events.send(event);
    }

    /// The worker's main loop: connect, serve until the connection ends, then either relaunch (restart request) or finish. [`KernelEvent::State`]`(Starting)` is re-emitted for every iteration so the status bar tracks restarts.
    pub(crate) async fn run(mut self, mut commands: UnboundedReceiver<Command>) {
        loop {
            self.emit(KernelEvent::State(KernelState::Starting));
            let next = match self.connect(&mut commands).await {
                Ok(mut sockets) => self.serve(&mut sockets, &mut commands).await,
                Err(next) => next,
            };
            match next {
                Next::Restart => {
                    if let Err(reason) = self.relaunch() {
                        self.finish(reason);
                        return;
                    }
                    self.emit(KernelEvent::Restarted);
                }
                Next::Stop(reason) => {
                    self.finish(reason);
                    return;
                }
            }
        }
    }

    /// Terminal path: drop `launched` (kills the process and removes the runtime directory) before announcing, flush any replies still held, then emit the final `Dead` state and reason.
    fn finish(&mut self, reason: DeathReason) {
        // Kill the process and remove the runtime directory before saying so.
        self.launched = None;
        self.flush_replies(true);
        self.emit(KernelEvent::State(KernelState::Dead));
        self.emit(KernelEvent::Dead(reason));
    }

    /// If we launched the kernel, whether its process has exited and why.
    fn check_exit(&mut self) -> Option<DeathReason> {
        self.launched.as_mut().and_then(Launched::exited)
    }

    /// Connect the three sockets, while watching for the process to exit,
    /// the start timeout, and a kill or shutdown from the handle.
    async fn connect(
        &mut self,
        commands: &mut UnboundedReceiver<Command>,
    ) -> Result<Sockets, Next> {
        let connecting = connect_sockets(
            self.info.endpoint(Channel::Shell),
            self.info.endpoint(Channel::Control),
            self.info.endpoint(Channel::IoPub),
            self.start_timeout,
        );
        tokio::pin!(connecting);
        let deadline = Instant::now() + self.start_timeout;
        let mut tick = interval(EXIT_POLL);
        loop {
            tokio::select! {
                result = &mut connecting => {
                    return result.map_err(|e| Next::Stop(DeathReason::Transport(e.to_string())));
                }
                command = commands.recv() => match command {
                    None | Some(Command::Kill | Command::Shutdown) => {
                        return Err(Next::Stop(DeathReason::Shutdown));
                    }
                    Some(other) => self.pending.push(other),
                },
                _ = tick.tick() => {
                    if let Some(reason) = self.check_exit() {
                        return Err(Next::Stop(reason));
                    }
                    if Instant::now() >= deadline {
                        return Err(Next::Stop(DeathReason::StartTimeout));
                    }
                }
            }
        }
    }

    /// A fresh message id: the session id plus a worker-local counter (`_w` marks worker-originated messages; `KernelHandle::execute` uses plain numbers).
    fn next_id(&mut self) -> String {
        self.counter += 1;
        format!("{}_w{}", self.session, self.counter)
    }

    /// Build, sign and send one message of type `msg_type`. `id` is supplied for `execute_request` (the App already holds it) and generated otherwise.
    async fn send<S: SocketSend + Send>(
        &mut self,
        socket: &mut S,
        msg_type: &str,
        content: Value,
        id: Option<String>,
    ) -> ZmqResult<()> {
        let id = id.unwrap_or_else(|| self.next_id());
        let date = jiff::Timestamp::now().to_string();
        let message = Message::new(msg_type, content, &id, &self.session, &self.username, &date);
        socket
            .send(to_zmq(wire::encode(&message, &self.signer)))
            .await
    }

    /// Authenticate and type one received message. A message that fails the signature or size checks is reported as [`KernelEvent::Rejected`] and dropped, never acted on.
    fn decode(&self, message: ZmqMessage) -> Option<(Message, Content)> {
        let frames = message.into_vec();
        let decoded = wire::decode(&frames, &self.signer, self.limits).and_then(|m| {
            let typed = m.typed().map_err(|_| DecodeError::Json("content"))?;
            Ok((m, typed))
        });
        match decoded {
            Ok(pair) => Some(pair),
            Err(e) => {
                self.emit(KernelEvent::Rejected(e));
                None
            }
        }
    }

    /// After a socket error: the process's exit if it has exited (give it a
    /// moment), or the error.
    async fn socket_failed(&mut self, error: &zeromq::ZmqError) -> Next {
        sleep(Duration::from_millis(200)).await;
        Next::Stop(
            self.check_exit()
                .unwrap_or_else(|| DeathReason::Transport(error.to_string())),
        )
    }

    /// Serve one connection: send `kernel_info_request`, then multiplex shell and iopub traffic, handle-side commands, the process exiting and the start deadline until the connection ends or a restart/stop is requested.
    async fn serve(&mut self, s: &mut Sockets, commands: &mut UnboundedReceiver<Command>) -> Next {
        if let Err(e) = self
            .send(&mut s.shell, "kernel_info_request", request::empty(), None)
            .await
        {
            return self.socket_failed(&e).await;
        }
        let mut ready = false;
        let deadline = Instant::now() + self.start_timeout;
        let mut tick = interval(EXIT_POLL);

        for command in std::mem::take(&mut self.pending) {
            if let Some(next) = self.command(s, Some(command)).await {
                return next;
            }
        }

        loop {
            tokio::select! {
                command = commands.recv() => {
                    if let Some(next) = self.command(s, command).await {
                        return next;
                    }
                }
                message = s.shell.recv() => match message {
                    Ok(message) => self.on_shell(message, &mut ready),
                    Err(e) => return self.socket_failed(&e).await,
                },
                message = s.iopub.recv() => match message {
                    Ok(message) => self.on_iopub(message),
                    Err(e) => return self.socket_failed(&e).await,
                },
                message = s.control.recv() => match message {
                    // Interrupt and shutdown replies: verified, otherwise unused.
                    Ok(message) => { let _ = self.decode(message); }
                    Err(e) => return self.socket_failed(&e).await,
                },
                _ = tick.tick() => {
                    self.flush_replies(false);
                    if let Some(reason) = self.check_exit() {
                        self.flush_replies(true);
                        return Next::Stop(reason);
                    }
                    if !ready && Instant::now() >= deadline {
                        return Next::Stop(DeathReason::StartTimeout);
                    }
                }
            }
        }
    }

    /// Handle one command; `Some` ends this kernel incarnation.
    async fn command(&mut self, s: &mut Sockets, command: Option<Command>) -> Option<Next> {
        let result = match command {
            Some(Command::Execute { id, code }) => {
                self.send(
                    &mut s.shell,
                    "execute_request",
                    request::execute(&code),
                    Some(id),
                )
                .await
            }
            Some(Command::Interrupt) => self.interrupt(s).await,
            Some(Command::Restart) => {
                if self.launched.is_none() {
                    // Not ours to relaunch: ask the kernel (its manager restarts it).
                    self.send(
                        &mut s.control,
                        "shutdown_request",
                        request::shutdown(true),
                        None,
                    )
                    .await
                } else {
                    self.emit(KernelEvent::State(KernelState::Restarting));
                    self.stop(s, true).await;
                    return Some(Next::Restart);
                }
            }
            Some(Command::Shutdown) => {
                self.stop(s, false).await;
                return Some(Next::Stop(DeathReason::Shutdown));
            }
            Some(Command::Kill) | None => return Some(Next::Stop(DeathReason::Shutdown)),
        };
        match result {
            Ok(()) => None,
            Err(e) => Some(self.socket_failed(&e).await),
        }
    }

    /// Interrupt the running request. For a kernel Vix launched whose kernelspec says `interrupt_mode: signal` on Unix, send SIGINT to the process; otherwise (message mode, Windows, or an attached kernel) send an `interrupt_request` on the control channel.
    async fn interrupt(&mut self, s: &mut Sockets) -> ZmqResult<()> {
        #[cfg(unix)]
        if let Some(launched) = &self.launched
            && launched.spec.interrupt_mode == vix_jupyter_core::InterruptMode::Signal
            && let Ok(pid) = i32::try_from(launched.child.id())
        {
            use nix::sys::signal::{Signal, kill};
            use nix::unistd::Pid;
            let _ = kill(Pid::from_raw(pid), Signal::SIGINT);
            return Ok(());
        }
        self.send(&mut s.control, "interrupt_request", request::empty(), None)
            .await
    }

    /// Ask the kernel to shut down, wait for the grace period, then kill a
    /// launched process that is still running.
    async fn stop(&mut self, s: &mut Sockets, restart: bool) {
        let _ = self
            .send(
                &mut s.control,
                "shutdown_request",
                request::shutdown(restart),
                None,
            )
            .await;
        let Some(launched) = self.launched.as_mut() else {
            // Not our process: wait (up to the grace period) for the
            // `shutdown_reply`, so the request is delivered before the
            // sockets close.
            let grace = self.grace;
            let _ = tokio::time::timeout(grace, async {
                while let Ok(message) = s.control.recv().await {
                    if let Some((_, Content::ShutdownReply(_))) = self.decode(message) {
                        break;
                    }
                }
            })
            .await;
            return;
        };
        let deadline = Instant::now() + self.grace;
        while Instant::now() < deadline {
            if !matches!(launched.child.try_wait(), Ok(None)) {
                return;
            }
            sleep(Duration::from_millis(20)).await;
        }
        let _ = launched.child.kill();
        let _ = launched.child.wait();
    }

    /// Start the launched kernel again on the same connection file.
    fn relaunch(&mut self) -> Result<(), DeathReason> {
        let Some(launched) = self.launched.as_mut() else {
            return Ok(());
        };
        RuntimeDir::clear_sockets(&self.info);
        let file = launched.dir.connection_file();
        let (child, stderr) = spawn_kernel(&launched.spec, &file, launched.cwd.as_deref())
            .map_err(|e| DeathReason::Transport(e.to_string()))?;
        launched.child = child;
        launched.stderr = stderr;
        Ok(())
    }

    /// Emit a pending reply as [`KernelEvent::Reply`].
    fn emit_reply(&self, held: HeldReply) {
        self.emit(KernelEvent::Reply {
            request: RequestId(held.request),
            status: held.status,
            execution_count: held.execution_count,
        });
    }

    /// Report held replies older than [`REPLY_WAIT`] (or all of them).
    fn flush_replies(&mut self, all: bool) {
        let now = Instant::now();
        let (due, keep): (Vec<_>, Vec<_>) = std::mem::take(&mut self.held)
            .into_iter()
            .partition(|h| all || now.duration_since(h.since) >= REPLY_WAIT);
        self.held = keep;
        for held in due {
            self.emit_reply(held);
        }
    }

    /// An `execute_reply` arrived: report it now if IOPub already said
    /// `idle` for its request, otherwise hold it until then.
    fn on_reply(&mut self, held: HeldReply) {
        if let Some(i) = self.idle.iter().position(|id| *id == held.request) {
            self.idle.swap_remove(i);
            self.emit_reply(held);
        } else {
            if self.held.len() >= MAX_TRACKED {
                let oldest = self.held.remove(0);
                self.emit_reply(oldest);
            }
            self.held.push(held);
        }
    }

    /// IOPub said `idle` for `request`: its outputs are all in.
    fn on_idle(&mut self, request: &str) {
        if let Some(i) = self.held.iter().position(|h| h.request == request) {
            let held = self.held.remove(i);
            self.emit_reply(held);
        } else {
            if self.idle.len() >= MAX_TRACKED {
                self.idle.remove(0);
            }
            self.idle.push(request.to_owned());
        }
    }

    /// Handle a shell message: `kernel_info_reply` marks the kernel ready; `execute_reply` is matched to its request via the parent id and held until the request's `idle` status arrives.
    fn on_shell(&mut self, message: ZmqMessage, ready: &mut bool) {
        let Some((message, content)) = self.decode(message) else {
            return;
        };
        match content {
            Content::KernelInfoReply(info) => {
                *ready = true;
                self.emit(KernelEvent::Ready(info));
            }
            Content::ExecuteReply(reply) => {
                if let Some(parent) = message.parent_id() {
                    self.on_reply(HeldReply {
                        request: parent.to_owned(),
                        status: reply.status,
                        execution_count: reply.execution_count,
                        since: Instant::now(),
                    });
                }
            }
            _ => {}
        }
    }

    /// Handle an IOPub message. `status` updates the kernel state (and, on `idle` for one of our `execute_request`s, releases the matching reply). Everything else is an output; IOPub is a broadcast to every client of the kernel, so only messages whose parent header carries this session's id are forwarded.
    fn on_iopub(&mut self, message: ZmqMessage) {
        let Some((message, content)) = self.decode(message) else {
            return;
        };
        if let Content::Status(status) = content {
            let state = match status.execution_state {
                ExecutionState::Busy => KernelState::Busy,
                ExecutionState::Idle => KernelState::Idle,
                ExecutionState::Starting => KernelState::Starting,
                ExecutionState::Unknown => return,
            };
            self.emit(KernelEvent::State(state));
            if state == KernelState::Idle
                && let Some(parent) = message.parent_header.as_ref()
                && parent.session == self.session
                && parent.msg_type == "execute_request"
            {
                self.on_idle(&parent.msg_id);
            }
            return;
        }
        // IOPub is a broadcast: keep only outputs of this session's requests.
        let Some(parent) = message.parent_header.as_ref() else {
            return;
        };
        if parent.session != self.session {
            return;
        }
        if let Some(output) = Output::from_content(content) {
            self.emit(KernelEvent::Output {
                request: RequestId(parent.msg_id.clone()),
                output,
            });
        }
    }
}
