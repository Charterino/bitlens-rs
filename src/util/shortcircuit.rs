#[macro_export]
macro_rules! some_or_return {
    ($item:expr) => {
        match $item {
            Some(v) => v,
            None => return,
        }
    };
}

#[macro_export]
macro_rules! some_or_break {
    ($item:expr) => {
        match $item {
            Some(v) => v,
            None => break,
        }
    };
}

#[macro_export]
macro_rules! ok_or_continue {
    ($item:expr) => {
        match $item {
            Ok(v) => v,
            Err(_) => continue,
        }
    };
}

#[macro_export]
macro_rules! ok_or_break {
    ($item:expr) => {
        match $item {
            Ok(v) => v,
            Err(_) => break,
        }
    };
}

#[macro_export]
macro_rules! ok_or_return {
    ($item:expr) => {
        match $item {
            Ok(v) => v,
            Err(_) => return,
        }
    };
}

pub use ok_or_break;
pub use ok_or_continue;
pub use ok_or_return;
pub use some_or_break;
pub use some_or_return;
