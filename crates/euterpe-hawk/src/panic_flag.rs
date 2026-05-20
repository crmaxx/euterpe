use std::cell::Cell;

thread_local! {
    static PANIC_ALREADY_REPORTED: Cell<bool> = const { Cell::new(false) };
}

/// Mark that the current thread panic was already reported (e.g. by Axum `CatchPanicLayer`).
pub fn mark_panic_reported() {
    PANIC_ALREADY_REPORTED.set(true);
}

/// Returns true if panic was already reported; resets the flag.
pub fn take_panic_already_reported() -> bool {
    PANIC_ALREADY_REPORTED.replace(false)
}
