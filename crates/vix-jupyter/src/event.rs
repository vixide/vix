//! What a kernel reports to the App.

use std::fmt;

use vix_jupyter_core::message::{KernelInfoReply, ReplyStatus};
use vix_jupyter_core::{DecodeError, Output};

/// The id of one `execute_request` (its `msg_id`). Outputs and the reply
/// carry it, so the App routes them to the code that asked.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RequestId(pub String);

impl fmt::Display for RequestId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// A kernel's state, for the status bar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum KernelState {
    /// Launched; not yet answering.
    Starting,
    /// Waiting for a request.
    Idle,
    /// Running a request.
    Busy,
    /// Being restarted.
    Restarting,
    /// Gone; no more events follow.
    Dead,
}

/// Why a kernel stopped.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum DeathReason {
    /// Shut down on request.
    Shutdown,
    /// The process exited on its own, with this code (if any), and the
    /// last few lines of its stderr with ANSI codes removed.
    Exited {
        /// The exit code, `None` when killed by a signal.
        code: Option<i32>,
        /// The tail of the kernel's stderr (at most 4 KiB).
        stderr: String,
    },
    /// No `kernel_info_reply` within the start timeout.
    StartTimeout,
    /// A ZeroMQ error (its text, which never contains the key).
    Transport(String),
}

/// One event from a kernel.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum KernelEvent {
    /// The kernel answered `kernel_info` and accepts requests. Sent after
    /// every start and restart.
    Ready(KernelInfoReply),
    /// The execution state changed.
    State(KernelState),
    /// An output of a request.
    Output {
        /// The request it belongs to.
        request: RequestId,
        /// The output.
        output: Output,
    },
    /// A request finished (`execute_reply`). Sent after the request's
    /// outputs: the reply is held until IOPub reports `idle` for the
    /// request (at most one second, in case IOPub dropped that status).
    Reply {
        /// The request.
        request: RequestId,
        /// How it ended.
        status: ReplyStatus,
        /// Its execution count.
        execution_count: Option<u64>,
    },
    /// The kernel was restarted; its state is gone.
    Restarted,
    /// A received message failed a check (size, framing, signature or
    /// JSON) and was dropped.
    Rejected(DecodeError),
    /// The kernel is gone. The last event.
    Dead(DeathReason),
}
