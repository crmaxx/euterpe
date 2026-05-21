//! Legacy module paths; implementations live under [`crate::source`].

#![allow(unused_imports)]

pub mod alac {
    pub use crate::source::alac::*;
}

pub mod ape {
    pub use crate::source::ape::*;
}

pub mod wav {
    pub use crate::source::wav::*;
}
