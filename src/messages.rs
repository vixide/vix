//! Right-drawer message browser: advice and notifications, each dismissable.

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Level {
    Info,
    Advice,
    Warn,
    Error,
}

pub struct Message {
    pub level: Level,
    pub text: String,
}

#[derive(Default)]
pub struct Messages {
    pub items: Vec<Message>,
    pub selected: usize,
}

impl Messages {
    pub fn push(&mut self, level: Level, text: impl Into<String>) {
        self.items.push(Message {
            level,
            text: text.into(),
        });
    }

    pub fn info(&mut self, text: impl Into<String>) {
        self.push(Level::Info, text);
    }

    pub fn advice(&mut self, text: impl Into<String>) {
        self.push(Level::Advice, text);
    }

    pub fn warn(&mut self, text: impl Into<String>) {
        self.push(Level::Warn, text);
    }

    pub fn error(&mut self, text: impl Into<String>) {
        self.push(Level::Error, text);
    }

    pub fn up(&mut self) {
        self.selected = self.selected.saturating_sub(1);
    }

    pub fn down(&mut self) {
        if self.selected + 1 < self.items.len() {
            self.selected += 1;
        }
    }

    /// Dismiss the selected message (the "close x").
    pub fn close_selected(&mut self) {
        if self.selected < self.items.len() {
            self.items.remove(self.selected);
            if self.selected >= self.items.len() {
                self.selected = self.items.len().saturating_sub(1);
            }
        }
    }
}
