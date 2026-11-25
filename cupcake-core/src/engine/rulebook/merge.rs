//! Merging builtin-generated signals into a rulebook.

use std::collections::hash_map::Entry;
use std::collections::HashMap;

use tracing::debug;

use super::{Rulebook, SignalConfig};

impl Rulebook {
    /// Merge builtin-generated signals, keeping any user-defined signal of the same name.
    pub(super) fn merge_builtin_signals(&mut self, builtin_signals: HashMap<String, SignalConfig>) {
        for (name, signal) in builtin_signals {
            match self.signals.entry(name) {
                Entry::Vacant(e) => {
                    debug!("Adding builtin-generated signal: {}", e.key());
                    e.insert(signal);
                }
                Entry::Occupied(e) => {
                    debug!(
                        "Keeping user-defined signal: {} (skipping builtin)",
                        e.key()
                    );
                }
            }
        }
    }
}
