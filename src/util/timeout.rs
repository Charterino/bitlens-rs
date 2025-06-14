#[macro_export]
macro_rules! with_deadline {
    ($future:expr, $deadline:expr) => {{
        select! {
            _ = tokio::time::sleep_until($deadline) => {
                Err(anyhow::anyhow!("deadline hit"))
            }
            result = $future => {
                // proceed with the result
                result
            }
        }
    }};
}

pub use with_deadline;
