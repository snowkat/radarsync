use std::ops::{Deref, DerefMut};

use indicatif::ProgressBar;

use crate::ProgressMode;

#[derive(Clone)]
pub struct Progression {
    bar: ProgressBar,
}

impl Progression {
    pub fn new_spinner(mode: ProgressMode, message: impl Into<String>) -> Self {
        let bar = if mode == ProgressMode::On {
            ProgressBar::new_spinner().with_message(message.into())
        } else {
            ProgressBar::hidden()
        };
        Self { bar }
    }

    pub fn new(mode: ProgressMode, len: u64, message: impl Into<String>) -> Self {
        let bar = if mode == ProgressMode::On {
            ProgressBar::new(len).with_message(message.into())
        } else {
            ProgressBar::hidden()
        };
        Self { bar }
    }
}

impl Deref for Progression {
    type Target = ProgressBar;

    fn deref(&self) -> &Self::Target {
        &self.bar
    }
}

impl DerefMut for Progression {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.bar
    }
}
