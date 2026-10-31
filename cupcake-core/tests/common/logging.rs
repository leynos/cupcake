//! Test logging initialisation shared by the integration tests that include it.

use std::sync::Once;

/// Initialize logging for tests (only once per test run)
static INIT: Once = Once::new();

pub fn init_test_logging() {
    INIT.call_once(|| {
        // Use tracing subscriber for tests since the engine uses tracing
        use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

        let _ = tracing_subscriber::registry()
            .with(
                tracing_subscriber::fmt::layer()
                    .with_test_writer()
                    .with_target(true)
                    .with_level(true)
                    .with_thread_ids(false)
                    .with_thread_names(false),
            )
            .with(tracing_subscriber::filter::EnvFilter::from_default_env())
            .try_init();
    });
}
