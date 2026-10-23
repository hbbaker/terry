//! The PreToolUse hook protocol: the call comes as JSON on stdin; an answer is one JSON line
//! on stdout. No output lets the call follow the normal permission rules.

use std::io::{self, Read};
use std::panic::{self, UnwindSafe};

use serde_json::{Value, json};

#[derive(Debug, Clone, Copy)]
pub enum Decision {
    /// The permission prompt asks the user, with the reason as its text.
    Ask,
    /// The call does not run; Claude reads the reason.
    Deny,
}

impl Decision {
    fn as_str(self) -> &'static str {
        match self {
            Decision::Ask => "ask",
            Decision::Deny => "deny",
        }
    }
}

/// Reads the tool call Claude Code sends on stdin.
pub fn read_call() -> Result<Value, String> {
    let mut input = String::new();
    io::stdin()
        .read_to_string(&mut input)
        .map_err(|error| error.to_string())?;
    serde_json::from_str(&input).map_err(|error| error.to_string())
}

pub fn decide(decision: Decision, reason: &str) {
    let answer = json!({
        "hookSpecificOutput": {
            "hookEventName": "PreToolUse",
            "permissionDecision": decision.as_str(),
            "permissionDecisionReason": reason,
        }
    });
    println!("{answer}");
}

/// Runs a hook's body. A panic in it becomes `on_panic(message)` rather than a crash with no
/// answer.
pub fn guard(body: impl FnOnce() + UnwindSafe, on_panic: impl FnOnce(&str)) {
    panic::set_hook(Box::new(|_| {}));
    if let Err(payload) = panic::catch_unwind(body) {
        let message = payload
            .downcast_ref::<&str>()
            .map(|text| (*text).to_owned())
            .or_else(|| payload.downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "unknown panic".to_owned());
        on_panic(&message);
    }
}
